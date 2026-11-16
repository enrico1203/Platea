package app.platea

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.navigation.compose.rememberNavController
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import app.platea.data.Api
import app.platea.data.ItemPage
import app.platea.data.LibraryPage
import app.platea.data.Session
import app.platea.data.SessionState
import app.platea.player.ControlsState
import app.platea.player.Panel
import app.platea.player.PlayerActivity
import app.platea.player.PlayerController
import app.platea.player.PlayerScreen
import app.platea.ui.LocalNav
import app.platea.ui.Main
import app.platea.ui.Nav
import app.platea.ui.components.LocalIsTv
import app.platea.ui.components.LocalWidthDp
import app.platea.ui.screens.ConnectScreen
import app.platea.ui.screens.ItemScreen
import app.platea.ui.screens.LibraryScreen
import app.platea.ui.screens.LoginScreen
import app.platea.ui.theme.PlateaTheme
import com.github.takahirom.roborazzi.captureRoboImage
import kotlinx.coroutines.runBlocking
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowLooper

/**
 * Schermate vere disegnate sulla JVM, collegate al server di prova di `scripts/android-test/contract.py --keep`:
 *
 *     scripts/gradle.sh testDebugUnitTest -PplateaServer=http://127.0.0.1:18196
 *
 * Le immagini finiscono in `app/build/screens/`. Senza server i test si saltano.
 */
@RunWith(AndroidJUnit4::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
abstract class ScreensBase(private val tv: Boolean, private val widthDp: Int, private val tag: String) {
    @get:Rule
    val compose = createComposeRule()

    private val server = System.getProperty("platea.server").orEmpty()

    @Before
    fun login() {
        assumeTrue("server di prova non indicato", server.isNotEmpty())
        val app = ApplicationProvider.getApplicationContext<android.app.Application>()
        Api.init(app, tv)
        Api.store.server = server
        Api.store.token = null
        runBlocking { Session.login("admin", "password-di-prova") }
        assumeTrue(Session.state.value is SessionState.LoggedIn)
    }

    private fun host(content: @Composable () -> Unit) {
        compose.setContent {
            PlateaTheme(forceDark = true) {
                CompositionLocalProvider(LocalIsTv provides tv, LocalWidthDp provides widthDp) {
                    val nav = Nav(rememberNavController(), LocalContext.current)
                    CompositionLocalProvider(LocalNav provides nav) { content() }
                }
            }
        }
    }

    /** Aspetta che le richieste al server e le immagini arrivino, poi salva la schermata. */
    private fun shoot(name: String) {
        repeat(30) {
            Thread.sleep(150)
            ShadowLooper.idleMainLooper()
            compose.waitForIdle()
        }
        compose.onRoot().captureRoboImage("build/screens/${tag}_$name.png")
    }

    private fun firstOf(path: String) = runBlocking { Api.get<ItemPage>(path).items.first() }
    private fun libraries() = runBlocking { Api.get<LibraryPage>("/api/v1/libraries").items }

    @Test
    fun home() { host { Main("home") }; shoot("home") }

    @Test
    fun libraries_() { host { Main("libraries") }; shoot("libraries") }

    @Test
    fun search() { host { Main("search") }; shoot("search") }

    @Test
    fun account() { host { Main("account") }; shoot("account") }

    @Test
    fun playlists() { host { Main("playlists") }; shoot("playlists") }

    @Test
    fun library() {
        val id = libraries().first { it.kind == "movies" }.id
        host { LibraryScreen(id) }
        shoot("library")
    }

    @Test
    fun movie() {
        val lib = libraries().first { it.kind == "movies" }.id
        val m = firstOf("/api/v1/items?library_id=$lib&sort=title&order=asc&limit=60")
        host { ItemScreen(m.id) }
        shoot("movie")
    }

    @Test
    fun series() {
        val lib = libraries().first { it.kind == "shows" }.id
        val s = firstOf("/api/v1/items?library_id=$lib&sort=title&order=asc&limit=60")
        host { ItemScreen(s.id) }
        shoot("series")
    }

    @Test
    fun login_() {
        val info = (Session.state.value as SessionState.LoggedIn).info
        host { LoginScreen(info) }
        shoot("login")
    }

    /** Player con i controlli e il pannello delle tracce: ExoPlayer non decodifica sulla JVM, ma la sessione è vera. */
    @Test
    fun player() {
        val lib = libraries().first { it.kind == "shows" }.id
        val series = firstOf("/api/v1/items?library_id=$lib&sort=title&order=asc&limit=60")
        val season = firstOf("/api/v1/items/${series.id}/children")
        val ep = firstOf("/api/v1/items/${season.id}/children")
        val ctx = ApplicationProvider.getApplicationContext<android.app.Application>()
        val controller = PlayerController(ctx, PlayerActivity.Args(ep.id, null, null, 0, false))
        val controls = ControlsState()
        host { PlayerScreen(controller, controls, onBack = {}) }
        shoot("player")
        controls.panel = Panel.Subtitles
        shoot("player_tracks")
        controller.release()
    }

    @Test
    fun connect() { host { ConnectScreen() }; shoot("connect") }
}

@Config(sdk = [34], qualifiers = "w411dp-h891dp-xxhdpi")
class PhoneScreensTest : ScreensBase(tv = false, widthDp = 411, tag = "phone")

@Config(sdk = [34], qualifiers = "w1280dp-h800dp-land-xhdpi")
class TabletScreensTest : ScreensBase(tv = false, widthDp = 1280, tag = "tablet")

@Config(sdk = [34], qualifiers = "w960dp-h540dp-land-television-xhdpi")
class TvScreensTest : ScreensBase(tv = true, widthDp = 960, tag = "tv")
