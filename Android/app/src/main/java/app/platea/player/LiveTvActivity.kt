package app.platea.player

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.view.KeyEvent
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.annotation.OptIn
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.lifecycleScope
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.okhttp.OkHttpDataSource
import androidx.media3.exoplayer.DefaultRenderersFactory
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.ui.PlayerView
import app.platea.R
import app.platea.data.Api
import app.platea.data.LiveChannel
import app.platea.data.LiveChannelPage
import app.platea.data.LivePlay
import app.platea.data.LivePlayRequest
import app.platea.ui.screens.ChannelLogo
import app.platea.ui.theme.PlateaTheme
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.text.DateFormat
import java.util.Date

/**
 * Live TV: il canale arriva in MPEG-TS attraverso il server (Media3 lo legge da sé, AC3 e MPEG-2 compresi se il
 * dispositivo li decodifica). Su e giù (o Canale +/−) cambiano canale; le informazioni compaiono per qualche secondo.
 */
@OptIn(UnstableApi::class)
class LiveTvActivity : ComponentActivity() {
    companion object {
        fun start(context: Context, channelId: String, order: List<String>) {
            context.startActivity(Intent(context, LiveTvActivity::class.java).putExtra("channel", channelId).putExtra("order", order.toTypedArray()))
        }
    }

    private lateinit var player: ExoPlayer
    private var channels by mutableStateOf<List<LiveChannel>>(emptyList())
    private var current by mutableStateOf("")
    private var session: String? = null
    private var error by mutableStateOf<String?>(null)
    private var buffering by mutableStateOf(true)
    private var infoUntil by mutableLongStateOf(0L)
    private var openJob: Job? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        WindowCompat.setDecorFitsSystemWindows(window, false)
        WindowInsetsControllerCompat(window, window.decorView).apply {
            hide(WindowInsetsCompat.Type.systemBars())
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        }
        player = ExoPlayer.Builder(this)
            .setRenderersFactory(DefaultRenderersFactory(this).setEnableDecoderFallback(true).setExtensionRendererMode(DefaultRenderersFactory.EXTENSION_RENDERER_MODE_PREFER))
            .setMediaSourceFactory(DefaultMediaSourceFactory(OkHttpDataSource.Factory(Api.http).setUserAgent(Api.userAgent)))
            .setAudioAttributes(AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MOVIE).build(), true)
            .build()
        player.addListener(object : Player.Listener {
            override fun onPlaybackStateChanged(state: Int) {
                buffering = state == Player.STATE_BUFFERING || state == Player.STATE_IDLE
                // Un flusso dal vivo non finisce: se "finisce" la sorgente si è interrotta, si riapre.
                if (state == Player.STATE_ENDED) open(current)
            }
            override fun onPlayerError(e: PlaybackException) {
                error = e.localizedMessage ?: e.errorCodeName
            }
        })
        current = intent.getStringExtra("channel") ?: ""
        lifecycleScope.launch {
            channels = runCatching { Api.get<LiveChannelPage>("/api/v1/livetv/channels").items }.getOrDefault(emptyList())
        }
        open(current)
        // La sessione resta viva; la guida si aggiorna.
        lifecycleScope.launch {
            while (true) {
                delay(20_000)
                session?.let { Api.fireAndForget("/api/v1/livetv/sessions/$it/ping", null) }
                channels = runCatching { Api.get<LiveChannelPage>("/api/v1/livetv/channels").items }.getOrDefault(channels)
            }
        }
        setContent {
            PlateaTheme(forceDark = true) { Screen() }
        }
    }

    private fun open(id: String) {
        if (id.isEmpty()) return
        current = id
        error = null
        infoUntil = System.currentTimeMillis() + 5000
        openJob?.cancel()
        openJob = lifecycleScope.launch {
            session?.let { Api.fireAndForget("/api/v1/livetv/sessions/$it/stop", null) }
            session = null
            player.stop()
            try {
                val r = Api.post<LivePlay>("/api/v1/livetv/channels/$id/play", LivePlayRequest(direct = true))
                session = r.sessionId
                val url = Api.url(r.streamUrl ?: r.hlsUrl) ?: return@launch
                val item = MediaItem.Builder().setUri(url).setMimeType(if (r.streamUrl != null) MimeTypes.VIDEO_MP2T else MimeTypes.APPLICATION_M3U8).build()
                player.setMediaItem(item)
                player.prepare()
                player.playWhenReady = true
            } catch (e: Exception) {
                error = Api.message(e, e.message ?: "")
            }
        }
    }

    private fun step(delta: Int) {
        val list = channels.ifEmpty { return }
        val i = list.indexOfFirst { it.id == current }.coerceAtLeast(0)
        open(list[(i + delta + list.size) % list.size].id)
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent?): Boolean {
        when (keyCode) {
            KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_CHANNEL_UP, KeyEvent.KEYCODE_PAGE_UP -> { step(-1); return true }
            KeyEvent.KEYCODE_DPAD_DOWN, KeyEvent.KEYCODE_CHANNEL_DOWN, KeyEvent.KEYCODE_PAGE_DOWN -> { step(1); return true }
            KeyEvent.KEYCODE_DPAD_CENTER, KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_INFO -> { infoUntil = System.currentTimeMillis() + 5000; return true }
        }
        return super.onKeyDown(keyCode, event)
    }

    override fun onStop() {
        super.onStop()
        player.pause()
    }

    override fun onStart() {
        super.onStart()
        if (session != null) player.play()
    }

    override fun onDestroy() {
        session?.let { Api.fireAndForget("/api/v1/livetv/sessions/$it/stop", null) }
        player.release()
        super.onDestroy()
    }

    @androidx.compose.runtime.Composable
    private fun Screen() {
        val time = DateFormat.getTimeInstance(DateFormat.SHORT)
        var now by androidx.compose.runtime.remember { mutableLongStateOf(System.currentTimeMillis()) }
        LaunchedEffect(Unit) { while (true) { delay(1000); now = System.currentTimeMillis() } }
        val ch = channels.firstOrNull { it.id == current }
        Box(Modifier.fillMaxSize().background(Color.Black).clickable { infoUntil = System.currentTimeMillis() + 5000 }) {
            AndroidView(
                factory = { ctx -> PlayerView(ctx).apply { useController = false; player = this@LiveTvActivity.player; keepScreenOn = true } },
                modifier = Modifier.fillMaxSize(),
            )
            if (buffering && error == null) {
                CircularProgressIndicator(Modifier.align(Alignment.Center), color = Color.White)
            }
            error?.let {
                Text(stringResource(R.string.tv_error, it), color = Color.White, modifier = Modifier.align(Alignment.Center).padding(32.dp))
            }
            if (now < infoUntil && ch != null) {
                Column(
                    Modifier.align(Alignment.BottomStart).fillMaxWidth()
                        .background(Brush.verticalGradient(listOf(Color.Transparent, Color(0xCC000000))))
                        .padding(horizontal = 32.dp, vertical = 24.dp),
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        ChannelLogo(ch, 56)
                        Column(Modifier.padding(start = 16.dp)) {
                            Text("${ch.numberText} ${ch.name}".trim(), style = MaterialTheme.typography.titleLarge, color = Color.White)
                            ch.now?.let { p ->
                                Text("${time.format(Date(p.startMs))}–${time.format(Date(p.endMs))}  ${p.title}", style = MaterialTheme.typography.bodyLarge, color = Color(0xFFC3C9D2), maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                            ch.next?.let { n ->
                                Text(stringResource(R.string.tv_next, time.format(Date(n.startMs)), n.title), style = MaterialTheme.typography.bodySmall, color = Color(0xFFA2A9B4), maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                        }
                    }
                }
            }
        }
    }
}
