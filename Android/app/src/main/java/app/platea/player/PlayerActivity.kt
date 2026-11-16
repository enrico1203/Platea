package app.platea.player

import android.app.PictureInPictureParams
import android.content.Context
import android.content.Intent
import android.content.pm.ActivityInfo
import android.content.res.Configuration
import android.os.Build
import android.os.Bundle
import android.util.Rational
import android.view.KeyEvent
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.annotation.OptIn
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.MediaSession
import app.platea.PlateaApp
import app.platea.ui.components.LocalIsTv
import app.platea.ui.theme.PlateaTheme

/** Stato dei controlli: visibili, pannello aperto, suggerimento di salto. */
class ControlsState {
    var visible by mutableStateOf(true)
    var panel by mutableStateOf(Panel.None)
    var seekHint by mutableStateOf<String?>(null)
    var seekHintLeft by mutableStateOf(false)
    var lastPoke by mutableLongStateOf(System.currentTimeMillis())
    var pip by mutableStateOf(false)

    fun poke() {
        visible = true
        lastPoke = System.currentTimeMillis()
    }

    fun hide() {
        visible = false
        panel = Panel.None
    }
}

@OptIn(UnstableApi::class)
class PlayerActivity : ComponentActivity() {
    data class Args(val itemId: String, val fileId: String?, val playlistId: String?, val playlistIndex: Int, val startOver: Boolean)

    companion object {
        fun start(context: Context, itemId: String, fileId: String?, playlistId: String?, index: Int, startOver: Boolean) {
            context.startActivity(
                Intent(context, PlayerActivity::class.java)
                    .putExtra("item", itemId).putExtra("file", fileId).putExtra("playlist", playlistId)
                    .putExtra("index", index).putExtra("startOver", startOver),
            )
        }

        private fun args(i: Intent) = Args(
            i.getStringExtra("item") ?: "", i.getStringExtra("file"), i.getStringExtra("playlist"),
            i.getIntExtra("index", 0), i.getBooleanExtra("startOver", false),
        )
    }

    private lateinit var controller: PlayerController
    private var session: MediaSession? = null
    private val controls = ControlsState()
    private var tv = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        tv = PlateaApp.isTv(this)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        WindowCompat.setDecorFitsSystemWindows(window, false)
        immersive()
        // Telefoni in orizzontale (si può girare tra i due versi); tablet e TV liberi.
        if (!tv && resources.configuration.smallestScreenWidthDp < 600) {
            requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
        }
        controller = PlayerController(this, args(intent))
        session = runCatching { MediaSession.Builder(this, controller.player).build() }.getOrNull()
        setContent {
            PlateaTheme(forceDark = true) {
                androidx.compose.runtime.CompositionLocalProvider(LocalIsTv provides tv) {
                    PlayerScreen(controller, controls, onBack = { finish() })
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        controller.replace(args(intent))
    }

    private fun immersive() {
        val c = WindowInsetsControllerCompat(window, window.decorView)
        c.hide(WindowInsetsCompat.Type.systemBars())
        c.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) immersive()
    }

    /** Telefono: uscendo dall'app durante la riproduzione il video continua in una finestrella. */
    override fun onUserLeaveHint() {
        super.onUserLeaveHint()
        if (!tv && Build.VERSION.SDK_INT >= 26 && controller.player.isPlaying &&
            packageManager.hasSystemFeature(android.content.pm.PackageManager.FEATURE_PICTURE_IN_PICTURE)
        ) {
            val size = controller.player.videoSize
            val ratio = if (size.width > 0 && size.height > 0) Rational(size.width, size.height) else Rational(16, 9)
            val clamped = when {
                ratio.toFloat() > 2.39f -> Rational(239, 100)
                ratio.toFloat() < 0.42f -> Rational(42, 100)
                else -> ratio
            }
            runCatching { enterPictureInPictureMode(PictureInPictureParams.Builder().setAspectRatio(clamped).build()) }
        }
    }

    override fun onPictureInPictureModeChanged(isInPictureInPictureMode: Boolean, newConfig: Configuration) {
        super.onPictureInPictureModeChanged(isInPictureInPictureMode, newConfig)
        controls.pip = isInPictureInPictureMode
        if (isInPictureInPictureMode) controls.hide()
    }

    override fun onStop() {
        super.onStop()
        // Fuori schermo e non in picture-in-picture: pausa (il televisore spento non deve continuare a riprodurre).
        val inPip = Build.VERSION.SDK_INT >= 24 && isInPictureInPictureMode
        if (!inPip) controller.player.pause()
    }

    override fun onDestroy() {
        session?.release()
        controller.release()
        super.onDestroy()
    }

    /**
     * Telecomando. Con i controlli nascosti: OK = play/pausa, sinistra/destra = salti, su/giù = controlli.
     * Con i controlli visibili il focus si muove tra i pulsanti. Tasti media sempre attivi.
     */
    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        if (event.action != KeyEvent.ACTION_DOWN) return super.dispatchKeyEvent(event)
        val c = controller
        when (event.keyCode) {
            KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE, KeyEvent.KEYCODE_SPACE, KeyEvent.KEYCODE_HEADSETHOOK -> { c.togglePlay(); controls.poke(); return true }
            KeyEvent.KEYCODE_MEDIA_PLAY -> { c.player.play(); controls.poke(); return true }
            KeyEvent.KEYCODE_MEDIA_PAUSE -> { c.player.pause(); controls.poke(); return true }
            KeyEvent.KEYCODE_MEDIA_FAST_FORWARD -> { skip(c.prefs.skipForwardSeconds); return true }
            KeyEvent.KEYCODE_MEDIA_REWIND -> { skip(-c.prefs.skipBackSeconds); return true }
            KeyEvent.KEYCODE_MEDIA_NEXT -> { if (c.hasNext) c.next(); return true }
            KeyEvent.KEYCODE_BACK, KeyEvent.KEYCODE_ESCAPE -> {
                if (controls.panel != Panel.None) { controls.panel = Panel.None; controls.poke(); return true }
                if (tv && controls.visible && c.error == null && c.upNext == null) { controls.hide(); return true }
                return super.dispatchKeyEvent(event)
            }
        }
        if (c.error != null || c.upNext != null) return super.dispatchKeyEvent(event)
        if (!controls.visible) {
            when (event.keyCode) {
                KeyEvent.KEYCODE_DPAD_CENTER, KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER -> { c.togglePlay(); controls.poke(); return true }
                KeyEvent.KEYCODE_DPAD_LEFT -> { skip(-c.prefs.skipBackSeconds); return true }
                KeyEvent.KEYCODE_DPAD_RIGHT -> { skip(c.prefs.skipForwardSeconds); return true }
                KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_DPAD_DOWN, KeyEvent.KEYCODE_MENU -> { controls.poke(); return true }
            }
        } else {
            controls.lastPoke = System.currentTimeMillis()
        }
        return super.dispatchKeyEvent(event)
    }

    private fun skip(seconds: Int) {
        controller.skip(seconds)
        controls.seekHint = (if (seconds > 0) "+" else "−") + "${kotlin.math.abs(seconds)} s"
        controls.seekHintLeft = seconds < 0
    }
}
