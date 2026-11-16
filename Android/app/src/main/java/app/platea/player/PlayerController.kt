package app.platea.player

import android.content.Context
import android.net.Uri
import androidx.annotation.OptIn
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.TrackSelectionOverride
import androidx.media3.common.Tracks
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.HttpDataSource
import androidx.media3.datasource.okhttp.OkHttpDataSource
import androidx.media3.exoplayer.DefaultRenderersFactory
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.exoplayer.upstream.DefaultLoadErrorHandlingPolicy
import app.platea.R
import app.platea.data.Api
import app.platea.data.Chapter
import app.platea.data.ChapterList
import app.platea.data.Item
import app.platea.data.ItemDetail
import app.platea.data.NextItem
import app.platea.data.PlaybackInfo
import app.platea.data.PlaybackInfoRequest
import app.platea.data.PlaylistDetail
import app.platea.data.ProgressRequest
import app.platea.data.Profile
import app.platea.data.Session
import app.platea.data.TracksRequest
import app.platea.data.UserPrefs
import app.platea.util.Fmt
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.serialization.json.jsonPrimitive

/** Voce successiva: episodio o voce della playlist. */
data class NextUp(val itemId: String, val title: String, val playlistIndex: Int?)

enum class Panel { None, Subtitles, Quality, Info, Chapters }

/**
 * Riproduzione di un titolo con le regole della web UI: il server decide il metodo dal profilo del dispositivo;
 * se il dispositivo rifiuta un direct play o un remux si riprova una volta con la transcodifica; una sessione chiusa
 * dal server (410: l'esecutore della transcodifica è caduto) si riapre dalla posizione corrente.
 */
@OptIn(UnstableApi::class)
class PlayerController(private val context: Context, start: PlayerActivity.Args) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    val prefs: UserPrefs = Session.me?.preferences ?: UserPrefs()

    val player: ExoPlayer = ExoPlayer.Builder(context)
        .setRenderersFactory(DefaultRenderersFactory(context).setEnableDecoderFallback(true))
        .setMediaSourceFactory(
            DefaultMediaSourceFactory(OkHttpDataSource.Factory(Api.http).setUserAgent(Api.userAgent))
                // La transcodifica può impiegare qualche secondo a produrre un segmento: più tentativi del normale.
                .setLoadErrorHandlingPolicy(DefaultLoadErrorHandlingPolicy(8)),
        )
        .setAudioAttributes(AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MOVIE).build(), true)
        .setHandleAudioBecomingNoisy(true)
        .setSeekBackIncrementMs(prefs.skipBackSeconds * 1000L)
        .setSeekForwardIncrementMs(prefs.skipForwardSeconds * 1000L)
        .build()

    // --- Stato osservato dall'interfaccia ---
    var args by mutableStateOf(start)
        private set
    var item by mutableStateOf<ItemDetail?>(null)
        private set
    var info by mutableStateOf<PlaybackInfo?>(null)
        private set
    var chapters by mutableStateOf<List<Chapter>>(emptyList())
        private set
    var playing by mutableStateOf(false)
        private set
    var buffering by mutableStateOf(true)
        private set
    var positionMs by mutableLongStateOf(0L)
        private set
    var bufferedMs by mutableLongStateOf(0L)
        private set
    var error by mutableStateOf<String?>(null)
        private set
    var errorDetail by mutableStateOf<String?>(null)
        private set
    var subIndex by mutableStateOf<Int?>(null)
        private set
    /** Qualità scelta nel player: null = automatica (preferenze dell'utente). 0 = originale. */
    var quality by mutableStateOf<Long?>(null)
        private set
    var upNext by mutableStateOf<NextUp?>(null)
        private set
    var countdown by mutableStateOf(0)
        private set
    var notice by mutableStateOf<String?>(null)
        private set
    var hasNext by mutableStateOf(false)
        private set

    val durationMs: Long get() = info?.durationMs?.takeIf { it > 0 } ?: player.duration.takeIf { it > 0 } ?: 0L

    private var sessionId: String? = null
    private var fallback: (() -> Unit)? = null
    private var reopen: (() -> Unit)? = null
    private var reopens = 0
    private var adminStopped = false
    /** Ultima posizione davvero riprodotta (ms): base del ripiego. */
    private var playedMs: Long? = null
    private var countdownJob: Job? = null
    private var currentStartSubtitle: Int? = null

    init {
        player.addListener(object : Player.Listener {
            override fun onIsPlayingChanged(isPlaying: Boolean) {
                playing = isPlaying
                sendProgress()
            }

            override fun onPlaybackStateChanged(state: Int) {
                buffering = state == Player.STATE_BUFFERING || state == Player.STATE_IDLE
                if (state == Player.STATE_ENDED) onEnded()
            }

            override fun onPlayerError(e: PlaybackException) = handleError(e)

            override fun onTracksChanged(tracks: Tracks) {
                // Il dispositivo non sa decodificare il video o l'audio che il profilo diceva di supportare:
                // ExoPlayer andrebbe avanti senza immagine o senza audio. Si passa alla transcodifica.
                val p = info ?: return
                if (p.method == "transcode" || tracks.isEmpty) return
                val videoKo = tracks.containsType(C.TRACK_TYPE_VIDEO) && !tracks.isTypeSupported(C.TRACK_TYPE_VIDEO)
                val audioKo = tracks.containsType(C.TRACK_TYPE_AUDIO) && !tracks.isTypeSupported(C.TRACK_TYPE_AUDIO)
                if (videoKo || audioKo) {
                    report(p.sessionId, "track not supported: video=${!videoKo} audio=${!audioKo}")
                    runFallback()
                    return
                }
                applySubtitle()
                applyAudioInFile()
            }
        })
        scope.launch { tick() }
        scope.launch { progressLoop() }
        scope.launch { Session.events.collect { onServerEvent(it) } }
        load(start)
    }

    // --- Avvio ---

    private fun load(a: PlayerActivity.Args) {
        args = a
        item = null
        info = null
        chapters = emptyList()
        error = null
        upNext = null
        countdownJob?.cancel()
        reopens = 0
        adminStopped = false
        scope.launch {
            try {
                val it = Api.get<ItemDetail>("/api/v1/items/${a.itemId}")
                item = it
                val resume = if (!a.startOver && it.userData.positionMs > 0 && !it.userData.played && a.fileId == null) it.userData.positionMs else 0L
                start(resume)
                hasNext = a.playlistId != null || it.kind == "episode"
            } catch (e: Exception) {
                error = Api.message(e, context.getString(R.string.error_network))
            }
        }
        scope.launch {
            chapters = runCatching {
                Api.get<ChapterList>("/api/v1/items/${a.itemId}/chapters" + (a.fileId?.let { "?file_id=$it" } ?: "")).items
            }.getOrDefault(emptyList())
        }
    }

    private fun profileBitrate(): Long? = quality?.takeIf { it > 0 }

    fun start(startMs: Long, audioIndex: Int? = null, subtitleIndex: Int? = null, force: Boolean = false) {
        error = null
        errorDetail = null
        stopSession(if (player.currentPosition > 0) player.currentPosition else null)
        val a = args
        scope.launch {
            try {
                val p = Api.post<PlaybackInfo>(
                    "/api/v1/playback/info",
                    PlaybackInfoRequest(
                        itemId = a.itemId,
                        fileId = a.fileId,
                        profile = Profile.build(context, profileBitrate()),
                        audioIndex = audioIndex,
                        subtitleIndex = subtitleIndex,
                        startMs = startMs,
                        forceTranscode = force,
                        uiLanguage = Fmt.uiLanguage(),
                        explicitQuality = quality != null,
                    ),
                )
                sessionId = p.sessionId
                info = p
                playedMs = null
                fallback = if (force || p.method == "transcode") null else {
                    { start(playedMs ?: startMs, p.audioIndex, subtitleIndex, force = true) }
                }
                reopen = { start(if (player.currentPosition > 0) player.currentPosition else startMs, p.audioIndex, subtitleIndex, force) }
                // Primo avvio: sottotitolo scelto dal server secondo le preferenze dell'utente.
                if (subtitleIndex == null && audioIndex == null) subIndex = p.subtitleDefault
                currentStartSubtitle = subtitleIndex
                attach(p, startMs)
            } catch (e: Exception) {
                error = Api.message(e, context.getString(R.string.error_network))
            }
        }
    }

    private fun attach(p: PlaybackInfo, startMs: Long) {
        val rel = p.hlsUrl ?: p.streamUrl
        val url = Api.url(rel)
        if (url == null) {
            error = context.getString(R.string.player_error_stream)
            return
        }
        // Sottotitoli di testo: WebVTT a parte, il video non viene toccato.
        val subs = p.subtitles.filter { it.url != null }.map { s ->
            MediaItem.SubtitleConfiguration.Builder(Uri.parse(Api.url(s.url)))
                .setId(subId(s.index))
                .setMimeType(MimeTypes.TEXT_VTT)
                .setLanguage(s.language)
                .setLabel(listOfNotNull(s.language?.let { Fmt.language(it) }, s.title).joinToString(" · ").ifEmpty { "#${s.index}" })
                .setSelectionFlags(if (s.forced) C.SELECTION_FLAG_FORCED else 0)
                .build()
        }
        val media = MediaItem.Builder()
            .setUri(url)
            .setMediaId(p.sessionId)
            .setSubtitleConfigurations(subs)
            .apply { if (p.hlsUrl != null) setMimeType(MimeTypes.APPLICATION_M3U8) }
            .build()
        player.setMediaItem(media, startMs)
        player.prepare()
        player.playWhenReady = true
        applySubtitle()
    }

    private fun subId(index: Int) = "platea-sub-$index"

    /** Sottotitolo attivo: solo quelli consegnati dal server; le tracce di testo del file restano spente. */
    private fun applySubtitle() {
        val idx = subIndex
        val p = info
        val builder = player.trackSelectionParameters.buildUpon()
        val want = if (idx == null || p == null || p.subtitle.delivery == "burn" && p.subtitle.index == idx) null else subId(idx)
        val group = want?.let { id ->
            player.currentTracks.groups.firstOrNull { g ->
                g.type == C.TRACK_TYPE_TEXT && (0 until g.length).any { i -> g.getTrackFormat(i).id.let { f -> f == id || f?.endsWith(":$id") == true } }
            }
        }
        if (group == null) {
            builder.setTrackTypeDisabled(C.TRACK_TYPE_TEXT, true).clearOverridesOfType(C.TRACK_TYPE_TEXT)
        } else {
            builder.setTrackTypeDisabled(C.TRACK_TYPE_TEXT, false)
                .setOverrideForType(TrackSelectionOverride(group.mediaTrackGroup, 0))
        }
        player.trackSelectionParameters = builder.build()
    }

    /** In direct play il file ha tutte le tracce audio: si sceglie quella indicata dal server senza riaprire. */
    private fun applyAudioInFile() {
        val p = info ?: return
        if (p.method != "direct_play") return
        val pos = p.audioTracks.indexOfFirst { it.index == p.audioIndex }
        if (pos < 0) return
        val groups = player.currentTracks.groups.filter { it.type == C.TRACK_TYPE_AUDIO }
        if (groups.size != p.audioTracks.size) return
        val g = groups[pos]
        if (g.isSelected) return
        player.trackSelectionParameters = player.trackSelectionParameters.buildUpon()
            .setOverrideForType(TrackSelectionOverride(g.mediaTrackGroup, 0)).build()
    }

    // --- Comandi ---

    fun togglePlay() {
        if (player.playbackState == Player.STATE_ENDED) player.seekTo(0)
        player.playWhenReady = !player.playWhenReady
    }

    fun seekTo(ms: Long) {
        val d = durationMs
        val target = if (d > 0) ms.coerceIn(0, d - 1000) else ms.coerceAtLeast(0)
        player.seekTo(target)
        positionMs = target
    }

    fun skip(seconds: Int) = seekTo(player.currentPosition + seconds * 1000L)

    fun selectSubtitle(index: Int?) {
        val p = info ?: return
        subIndex = index
        rememberTracks(p.audioIndex, index)
        val s = p.subtitles.firstOrNull { it.index == index }
        // Un sottotitolo a immagine va impresso nel video: serve una nuova decisione.
        if (s != null && s.image) {
            start(player.currentPosition, p.audioIndex, s.index)
            return
        }
        // Si toglie un sottotitolo impresso: nuova decisione senza.
        if (index == null && p.subtitle.delivery == "burn") {
            start(player.currentPosition, p.audioIndex, null)
            return
        }
        applySubtitle()
    }

    fun selectAudio(index: Int) {
        if (info == null) return
        rememberTracks(index, subIndex)
        start(player.currentPosition, index, currentStartSubtitle)
    }

    fun selectQuality(q: Long?) {
        quality = q
        start(player.currentPosition, info?.audioIndex, currentStartSubtitle)
    }

    fun retry() = start(if (positionMs > 0) positionMs else player.currentPosition, info?.audioIndex, currentStartSubtitle, force = true)

    /** Le tracce scelte valgono anche per la prossima volta (per il film o per tutta la serie). */
    private fun rememberTracks(audio: Int?, sub: Int?) {
        val sid = sessionId ?: return
        if (!prefs.rememberTracks) return
        Api.fireAndForget("/api/v1/playback/$sid/tracks", TracksRequest(audio, sub, sub == null))
    }

    // --- Errori ---

    private fun httpCode(e: Throwable?): Int? {
        var c: Throwable? = e
        while (c != null) {
            if (c is HttpDataSource.InvalidResponseCodeException) return c.responseCode
            c = c.cause
        }
        return null
    }

    private fun handleError(e: PlaybackException) {
        val p = info ?: return
        val code = httpCode(e)
        if (code == 410 && !adminStopped && reopens < 5) {
            reopens++
            reopen?.invoke()
            return
        }
        val detail = listOfNotNull(e.errorCodeName, code?.let { "HTTP $it" }, e.cause?.message).joinToString(" · ")
        report(p.sessionId, detail)
        if (p.method != "transcode" && fallback != null) {
            runFallback()
            return
        }
        errorDetail = detail
        error = context.getString(R.string.player_error_stream)
    }

    private fun runFallback() {
        val f = fallback ?: return
        fallback = null
        f()
    }

    private fun report(sid: String, message: String) {
        Api.fireAndForget("/api/v1/playback/$sid/client-error", ErrorBody(message.take(500)))
    }

    @kotlinx.serialization.Serializable
    data class ErrorBody(val message: String)

    // --- Sessione ---

    private fun stopSession(position: Long?) {
        val sid = sessionId ?: return
        sessionId = null
        Api.fireAndForget("/api/v1/playback/$sid/stop", ProgressRequest(position ?: 0))
    }

    private fun sendProgress() {
        val sid = sessionId ?: return
        Api.fireAndForget("/api/v1/playback/$sid/progress", ProgressRequest(player.currentPosition, !player.isPlaying))
    }

    private suspend fun progressLoop() {
        while (scope.isActive) {
            delay(10_000)
            sendProgress()
        }
    }

    /** Aggiorna posizione e buffer per l'interfaccia. */
    private suspend fun tick() {
        while (scope.isActive) {
            positionMs = player.currentPosition
            bufferedMs = player.bufferedPosition
            if (player.isPlaying && player.playbackState == Player.STATE_READY) playedMs = player.currentPosition
            delay(250)
        }
    }

    private fun onServerEvent(e: app.platea.data.ServerEvent) {
        val d = e.data ?: return
        val sid = d["session_id"]?.jsonPrimitive?.content
        if (sid == null || sid != info?.sessionId) return
        when (e.type) {
            "session.command" -> when (d["command"]?.jsonPrimitive?.content) {
                "pause" -> player.pause()
                "resume" -> player.play()
                "message" -> {
                    val from = d["from"]?.jsonPrimitive?.content ?: ""
                    val text = d["text"]?.jsonPrimitive?.content ?: return
                    notice = "$from: $text"
                    scope.launch { delay(10_000); notice = null }
                }
            }
            "playback.stopped" -> if (d["by"]?.jsonPrimitive?.content == "admin") {
                adminStopped = true
                player.pause()
                error = context.getString(R.string.player_stopped_by_admin)
            }
        }
    }

    // --- Voce successiva ---

    private suspend fun findNext(): NextUp? {
        val a = args
        if (a.playlistId != null) {
            val pl = Api.get<PlaylistDetail>("/api/v1/playlists/${a.playlistId}")
            val n = pl.items.getOrNull(a.playlistIndex + 1) ?: return null
            return NextUp(n.id, if (n.kind == "episode") "${n.seriesTitle ?: ""} · ${n.title}" else n.title, a.playlistIndex + 1)
        }
        val it = item ?: return null
        if (it.kind != "episode") return null
        val n: Item = Api.get<NextItem>("/api/v1/items/${it.id}/next").item ?: return null
        return NextUp(n.id, "${n.episodeCode} · ${n.title}", null)
    }

    fun next() {
        scope.launch { runCatching { findNext() }.getOrNull()?.let { playNext(it) } }
    }

    private fun onEnded() {
        stopSession(info?.durationMs)
        if (!prefs.autoplayNext) return
        scope.launch {
            val n = runCatching { findNext() }.getOrNull() ?: return@launch
            if (prefs.autoplayCountdownSeconds <= 0) {
                playNext(n)
                return@launch
            }
            upNext = n
            countdown = prefs.autoplayCountdownSeconds
            countdownJob?.cancel()
            countdownJob = scope.launch {
                while (countdown > 0) { delay(1000); countdown-- }
                upNext?.let { playNext(it) }
            }
        }
    }

    fun playNext(n: NextUp) {
        countdownJob?.cancel()
        upNext = null
        stopSession(player.currentPosition)
        player.stop()
        player.clearMediaItems()
        val a = args
        load(PlayerActivity.Args(n.itemId, null, a.playlistId, n.playlistIndex ?: a.playlistIndex, false))
    }

    fun cancelUpNext() {
        countdownJob?.cancel()
        upNext = null
    }

    /** Nuovo titolo chiesto mentre il player è già aperto (singleTask). */
    fun replace(a: PlayerActivity.Args) {
        stopSession(player.currentPosition)
        player.stop()
        player.clearMediaItems()
        quality = null
        load(a)
    }

    fun release() {
        stopSession(player.currentPosition)
        scope.cancel()
        player.release()
        Session.bumpCatalog()
    }
}
