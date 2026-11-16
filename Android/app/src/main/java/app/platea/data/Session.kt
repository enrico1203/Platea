package app.platea.data

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener

/** Stato dell'avvio: prima si cerca il server, poi l'utente. */
sealed interface SessionState {
    data object Loading : SessionState
    data object NoServer : SessionState
    data class Unreachable(val server: String) : SessionState
    data class LoggedOut(val info: SystemInfo) : SessionState
    data class LoggedIn(val info: SystemInfo, val me: Me) : SessionState
}

object Session {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val _state = MutableStateFlow<SessionState>(SessionState.Loading)
    val state: StateFlow<SessionState> = _state

    /** Cresce quando i dati del catalogo o dell'utente cambiano sul server: le schermate ricaricano. */
    private val _catalogTick = MutableStateFlow(0)
    val catalogTick: StateFlow<Int> = _catalogTick
    private val _events = MutableSharedFlow<ServerEvent>(extraBufferCapacity = 32)
    val events: SharedFlow<ServerEvent> = _events

    val me: Me? get() = (state.value as? SessionState.LoggedIn)?.me
    val info: SystemInfo? get() = when (val s = state.value) {
        is SessionState.LoggedIn -> s.info
        is SessionState.LoggedOut -> s.info
        else -> null
    }

    fun start() {
        Api.onUnauthorized = { scope.launch { reload() } }
        scope.launch { reload() }
    }

    suspend fun reload() {
        if (Api.store.server.isBlank()) {
            _state.value = SessionState.NoServer
            return
        }
        _state.value = SessionState.Loading
        val info = try {
            Api.get<SystemInfo>("/api/v1/system/info")
        } catch (e: Exception) {
            _state.value = SessionState.Unreachable(Api.store.server)
            return
        }
        val me = if (Api.store.token != null) runCatching { Api.get<Me>("/api/v1/me") }.getOrNull() else null
        _state.value = if (me != null) SessionState.LoggedIn(info, me) else SessionState.LoggedOut(info)
        if (me != null) connectEvents() else disconnectEvents()
    }

    /** Rilegge l'utente (preferenze cambiate) senza schermata di caricamento. */
    suspend fun refreshMe() {
        val s = state.value as? SessionState.LoggedIn ?: return
        runCatching { Api.get<Me>("/api/v1/me") }.onSuccess { _state.value = SessionState.LoggedIn(s.info, it) }
    }

    fun setPrefs(p: UserPrefs) {
        val s = state.value as? SessionState.LoggedIn ?: return
        _state.value = SessionState.LoggedIn(s.info, s.me.copy(preferences = p))
    }

    /** Prova un indirizzo: se risponde come un server Platea lo salva. */
    suspend fun connect(raw: String): Result<SystemInfo> {
        val candidates = normalize(raw)
        var last: Exception? = null
        for (base in candidates) {
            try {
                val text = Api.send("/api/v1/system/info", "GET", null, baseOverride = base)
                val info = Api.json.decodeFromString(SystemInfo.serializer(), text)
                Api.store.server = base
                Api.store.recentServers = listOf(base) + Api.store.recentServers
                Api.store.token = null
                _state.value = SessionState.LoggedOut(info)
                return Result.success(info)
            } catch (e: Exception) {
                last = e
            }
        }
        return Result.failure(last ?: ApiException(0, "BadUrl", ""))
    }

    /** "casa.local" → https://casa.local, http://casa.local:8096, http://casa.local:8080… */
    private fun normalize(raw: String): List<String> {
        val s = raw.trim().trimEnd('/')
        if (s.isEmpty()) return emptyList()
        if (s.startsWith("http://") || s.startsWith("https://")) return listOf(s)
        val hasPort = Regex(":\\d+$").containsMatchIn(s.substringBefore('/'))
        return if (hasPort) listOf("http://$s", "https://$s") else listOf("https://$s", "http://$s:8096", "http://$s:8080", "http://$s")
    }

    suspend fun login(username: String, password: String) {
        val r = Api.post<LoginResponse>(
            "/api/v1/auth/login",
            LoginBody(username, password, Api.store.deviceId, Api.deviceName, Api.clientName, app.platea.BuildConfig.VERSION_NAME),
        )
        acceptToken(r.token)
    }

    suspend fun acceptToken(token: String) {
        Api.store.token = token
        reload()
    }

    suspend fun logout() {
        runCatching { Api.postUnit("/api/v1/auth/logout") }
        Api.store.token = null
        disconnectEvents()
        reload()
    }

    fun forgetServer() {
        Api.store.token = null
        Api.store.server = ""
        disconnectEvents()
        _state.value = SessionState.NoServer
    }

    fun bumpCatalog() { _catalogTick.value++ }

    // --- Eventi in tempo reale (WebSocket) ---

    private var ws: WebSocket? = null
    private var wanted = false
    private var retry = 0
    private var retryJob: Job? = null
    private var flushJob: Job? = null

    private fun connectEvents() {
        wanted = true
        if (ws == null) open()
    }

    fun disconnectEvents() {
        wanted = false
        retryJob?.cancel()
        ws?.close(1000, null)
        ws = null
    }

    /** L'app torna in primo piano: riapre il canale se era caduto. */
    fun onForeground() {
        if (me != null) connectEvents()
    }

    private fun open() {
        val token = Api.store.token ?: return
        if (!wanted) return
        val base = Api.store.server.replaceFirst("https://", "wss://").replaceFirst("http://", "ws://")
        val req = runCatching { Request.Builder().url("$base/api/v1/events?token=${Api.enc(token)}").build() }.getOrNull() ?: return
        ws = Api.http.newWebSocket(req, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) { retry = 0 }
            override fun onMessage(webSocket: WebSocket, text: String) {
                val e = runCatching { Api.json.decodeFromString(ServerEvent.serializer(), text) }.getOrNull() ?: return
                scope.launch { handle(e) }
            }
            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) { scope.launch { closed(webSocket) } }
            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) { scope.launch { closed(webSocket) } }
        })
    }

    private fun closed(socket: WebSocket) {
        if (ws !== socket) return
        ws = null
        if (!wanted) return
        retryJob?.cancel()
        val wait = minOf(30_000L, 1000L shl minOf(retry++, 5))
        retryJob = scope.launch { delay(wait); if (wanted && ws == null) open() }
    }

    private fun handle(e: ServerEvent) {
        _events.tryEmit(e)
        // Raffiche di eventi (scansioni): si ricarica al massimo una volta ogni pochi secondi.
        val wait = when {
            e.type == "user_data.changed" -> 1_500L
            e.type.startsWith("library.") || e.type.startsWith("item.") -> 10_000L
            e.type == "resync" -> 0L
            else -> return
        }
        if (flushJob?.isActive == true) return
        flushJob = scope.launch { delay(wait); bumpCatalog() }
    }
}

@kotlinx.serialization.Serializable
data class LoginBody(
    val username: String,
    val password: String,
    @kotlinx.serialization.SerialName("device_id") val deviceId: String,
    @kotlinx.serialization.SerialName("device_name") val deviceName: String,
    val client: String,
    @kotlinx.serialization.SerialName("client_version") val clientVersion: String,
)

@kotlinx.serialization.Serializable
data class QuickConnectBody(
    @kotlinx.serialization.SerialName("device_id") val deviceId: String,
    @kotlinx.serialization.SerialName("device_name") val deviceName: String,
    val client: String,
    @kotlinx.serialization.SerialName("client_version") val clientVersion: String,
)
