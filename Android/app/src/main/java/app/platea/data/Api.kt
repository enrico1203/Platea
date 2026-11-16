package app.platea.data

import android.content.Context
import android.content.SharedPreferences
import android.os.Build
import app.platea.BuildConfig
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlinx.serialization.KSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.serializer
import okhttp3.Call
import okhttp3.Callback
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import java.io.IOException
import java.net.URLEncoder
import java.security.SecureRandom
import java.util.concurrent.TimeUnit
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/** Errore dell'API: `status` 0 = rete. Il messaggio arriva dal server, già tradotto nella sua lingua. */
class ApiException(val status: Int, val code: String, message: String) : IOException(message)

/** Indirizzo del server, token e identificativo del dispositivo. Il token resta solo sul dispositivo. */
class Store(context: Context) {
    private val prefs: SharedPreferences = context.getSharedPreferences("platea", Context.MODE_PRIVATE)

    var server: String
        get() = prefs.getString("server", "") ?: ""
        set(v) = prefs.edit().putString("server", v.trim().trimEnd('/')).apply()

    var token: String?
        get() = prefs.getString("token", null)
        set(v) = prefs.edit().apply { if (v == null) remove("token") else putString("token", v) }.apply()

    /** Ultimi server usati, il più recente per primo. */
    var recentServers: List<String>
        get() = (prefs.getString("recent_servers", "") ?: "").split('\n').filter { it.isNotBlank() }
        set(v) = prefs.edit().putString("recent_servers", v.distinct().take(5).joinToString("\n")).apply()

    val deviceId: String
        get() {
            prefs.getString("device_id", null)?.let { return it }
            val bytes = ByteArray(12).also { SecureRandom().nextBytes(it) }
            val id = bytes.joinToString("") { "%02x".format(it) }
            prefs.edit().putString("device_id", id).apply()
            return id
        }
}

object Api {
    val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        coerceInputValues = true
        encodeDefaults = true
    }

    lateinit var store: Store
        private set
    lateinit var http: OkHttpClient
        private set
    var isTv = false
    private val JSON_TYPE = "application/json".toMediaType()

    /** Chiamato quando il server rifiuta il token (sessione revocata, account cancellato). */
    var onUnauthorized: (() -> Unit)? = null

    fun init(context: Context, tv: Boolean) {
        store = Store(context.applicationContext)
        isTv = tv
        http = OkHttpClient.Builder()
            .connectTimeout(15, TimeUnit.SECONDS)
            .readTimeout(70, TimeUnit.SECONDS)
            .writeTimeout(30, TimeUnit.SECONDS)
            .pingInterval(30, TimeUnit.SECONDS)
            .addInterceptor { chain ->
                val req = chain.request()
                val token = store.token
                // Il token va solo al nostro server, mai a indirizzi esterni (immagini o trailer).
                val sameHost = store.server.toHttpUrlOrNull()?.host == req.url.host
                val out = if (token != null && sameHost && req.header("Authorization") == null) {
                    req.newBuilder().header("Authorization", "Bearer $token").build()
                } else req
                chain.proceed(out.newBuilder().header("User-Agent", userAgent).build())
            }
            .build()
    }

    val userAgent: String get() = "Platea-Android/${BuildConfig.VERSION_NAME} (${Build.MANUFACTURER} ${Build.MODEL}; Android ${Build.VERSION.RELEASE})"
    val clientName: String get() = if (isTv) "Platea Android TV" else "Platea Android"
    val deviceName: String
        get() {
            val m = Build.MODEL ?: "Android"
            val brand = Build.MANUFACTURER?.replaceFirstChar { it.uppercase() } ?: ""
            return if (m.startsWith(brand, ignoreCase = true) || brand.isEmpty()) m else "$brand $m"
        }

    /** URL assoluto per media e immagini. */
    fun url(path: String?): String? {
        if (path.isNullOrEmpty()) return null
        if (path.startsWith("http://") || path.startsWith("https://")) return path
        return store.server + path
    }

    /** Immagine ridimensionata dal server (`?w=`). */
    fun image(path: String?, width: Int): String? {
        val u = url(path) ?: return null
        return u + (if ('?' in u) "&" else "?") + "w=$width"
    }

    fun enc(s: String): String = URLEncoder.encode(s, "UTF-8")

    // --- Chiamate generiche ---

    suspend inline fun <reified T> get(path: String): T = call(path, "GET", null, serializer())
    suspend inline fun <reified T> post(path: String, body: Any? = null): T = call(path, "POST", body, serializer())
    suspend inline fun <reified T> put(path: String, body: Any? = null): T = call(path, "PUT", body, serializer())
    suspend inline fun <reified T> patch(path: String, body: Any? = null): T = call(path, "PATCH", body, serializer())
    suspend fun delete(path: String) { send(path, "DELETE", null) }
    suspend fun postUnit(path: String, body: Any? = null) { send(path, "POST", body) }

    suspend fun <T> call(path: String, method: String, body: Any?, ser: KSerializer<T>): T {
        val text = send(path, method, body)
        return withContext(Dispatchers.Default) { json.decodeFromString(ser, text.ifEmpty { "{}" }) }
    }

    @Suppress("UNCHECKED_CAST")
    private fun encodeBody(body: Any): String = when (body) {
        is String -> body
        else -> json.encodeToString(serializer(body::class.java) as KSerializer<Any>, body)
    }

    suspend fun send(path: String, method: String, body: Any?, baseOverride: String? = null): String {
        val base = baseOverride ?: store.server
        val payload = when {
            body != null -> encodeBody(body).toRequestBody(JSON_TYPE)
            method == "POST" || method == "PUT" || method == "PATCH" -> "".toRequestBody(JSON_TYPE)
            else -> null
        }
        val req = try {
            Request.Builder().url(base + path).header("Accept", "application/json").method(method, payload).build()
        } catch (e: IllegalArgumentException) {
            throw ApiException(0, "BadUrl", e.message ?: "")
        }
        val res = try {
            http.newCall(req).await()
        } catch (e: IOException) {
            throw ApiException(0, "Network", e.message ?: "")
        }
        return res.use { r ->
            val text = withContext(Dispatchers.IO) { r.body?.string() ?: "" }
            if (r.code == 401 && store.token != null && !path.startsWith("/api/v1/auth/login")) {
                store.token = null
                onUnauthorized?.invoke()
            }
            if (!r.isSuccessful) {
                val err = runCatching { json.decodeFromString(ApiErrorBody.serializer(), text) }.getOrNull()
                throw ApiException(r.code, err?.error ?: "HTTP${r.code}", err?.message ?: r.message)
            }
            text
        }
    }

    /** Messaggio da mostrare per un errore. */
    fun message(e: Throwable, network: String): String = when (e) {
        is ApiException -> if (e.status == 0 || e.message.isNullOrBlank()) network else e.message!!
        else -> e.message ?: network
    }

    /** Invio senza attesa del risultato (avanzamento, stop): non deve mai bloccare l'interfaccia. */
    fun fireAndForget(path: String, body: Any?) {
        val payload = (if (body != null) encodeBody(body) else "{}").toRequestBody(JSON_TYPE)
        val req = runCatching { Request.Builder().url(store.server + path).post(payload).build() }.getOrNull() ?: return
        http.newCall(req).enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {}
            override fun onResponse(call: Call, response: Response) { response.close() }
        })
    }
}

suspend fun Call.await(): Response = suspendCancellableCoroutine { cont ->
    enqueue(object : Callback {
        override fun onFailure(call: Call, e: IOException) { if (cont.isActive) cont.resumeWithException(e) }
        override fun onResponse(call: Call, response: Response) { cont.resume(response) }
    })
    cont.invokeOnCancellation { runCatching { cancel() } }
}
