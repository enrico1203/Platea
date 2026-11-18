package app.platea.ui

import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.windowInsetsTopHeight
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Brush
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.PlaylistPlay
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.VideoLibrary
import androidx.compose.ui.draw.drawBehind
import androidx.compose.material.icons.outlined.Home
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.VideoLibrary
import androidx.compose.material.icons.outlined.Person
import androidx.compose.material.icons.automirrored.outlined.PlaylistPlay
import androidx.compose.ui.geometry.Offset
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.NavigationRailItemDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.navigation.NavHostController
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument
import app.platea.PlateaApp
import app.platea.R
import app.platea.data.Api
import app.platea.data.Session
import app.platea.data.SessionState
import app.platea.player.PlayerActivity
import app.platea.ui.components.LocalIsTv
import app.platea.ui.components.LocalWidthDp
import app.platea.ui.components.Mark
import app.platea.ui.screens.AboutScreen
import app.platea.ui.screens.AccountScreen
import app.platea.ui.screens.BrowseScreen
import app.platea.ui.screens.ConnectScreen
import app.platea.ui.screens.HomeScreen
import app.platea.ui.screens.ItemScreen
import app.platea.ui.screens.LibrariesScreen
import app.platea.ui.screens.LibraryScreen
import app.platea.ui.screens.LoadingScreen
import app.platea.ui.screens.LoginScreen
import app.platea.ui.screens.PersonScreen
import app.platea.ui.screens.PlaylistScreen
import app.platea.ui.screens.PlaylistsScreen
import app.platea.ui.screens.SearchScreen
import app.platea.ui.screens.UnreachableScreen
import app.platea.ui.theme.PlateaTheme
import app.platea.ui.theme.tokens

/** Navigazione tra le schermate, disponibile ovunque. */
class Nav(private val nav: NavHostController, private val context: Context) {
    fun item(id: String) = nav.navigate("item/$id")
    fun library(id: String) = nav.navigate("library/$id")
    fun person(id: String) = nav.navigate("person/$id")
    fun playlist(id: String) = nav.navigate("playlist/$id")
    fun genre(name: String) = nav.navigate("browse?genre=${Api.enc(name)}")
    fun studio(name: String) = nav.navigate("browse?studio=${Api.enc(name)}")
    fun about() = nav.navigate("about")
    fun tv() = nav.navigate("tv")
    fun back() { nav.popBackStack() }
    fun tab(route: String) = nav.navigate(route) {
        popUpTo("home") { saveState = true }
        launchSingleTop = true
        restoreState = true
    }

    /** Il player è un'attività a sé: schermo intero, picture-in-picture, orientamento libero. */
    fun play(itemId: String, fileId: String? = null, playlistId: String? = null, index: Int = 0, startOver: Boolean = false) =
        PlayerActivity.start(context, itemId, fileId, playlistId, index, startOver)
}

val LocalNav = staticCompositionLocalOf<Nav> { error("Nav non disponibile") }

@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
@Composable
fun Root() {
    val context = LocalContext.current
    val state by Session.state.collectAsState()
    val tv = remember { PlateaApp.isTv(context) }
    val theme by Session.theme.collectAsState()
    val width = LocalConfiguration.current.screenWidthDp
    PlateaTheme(theme = theme, forceDark = tv) {
        CompositionLocalProvider(
            LocalIsTv provides tv,
            LocalWidthDp provides width,
            androidx.compose.foundation.gestures.LocalBringIntoViewSpec provides
                if (tv) TvBringIntoView else androidx.compose.foundation.gestures.LocalBringIntoViewSpec.current,
        ) {
            Box(Modifier.fillMaxSize().background(tokens.bg)) {
                when (val s = state) {
                    SessionState.Loading -> LoadingScreen()
                    SessionState.NoServer -> ConnectScreen()
                    is SessionState.Unreachable -> UnreachableScreen(s.server)
                    is SessionState.LoggedOut -> LoginScreen(s.info)
                    is SessionState.LoggedIn -> Main()
                }
            }
        }
    }
}

/**
 * Scorrimento col telecomando. Quello predefinito di Compose sulle TV tiene l'elemento a fuoco su un punto fisso
 * dello schermo e scorre anche quando è già tutto visibile: la parte alta delle schede (sfondo, locandina, titolo)
 * finiva fuori schermo appena il focus andava su Riproduci. Qui si scorre solo quanto serve, con un margine del 10%.
 */
@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
private val TvBringIntoView = object : androidx.compose.foundation.gestures.BringIntoViewSpec {
    @Suppress("OVERRIDE_DEPRECATION")
    override val scrollAnimationSpec: androidx.compose.animation.core.AnimationSpec<Float> =
        androidx.compose.animation.core.tween(durationMillis = 180, easing = androidx.compose.animation.core.FastOutSlowInEasing)

    override fun calculateScrollDistance(offset: Float, size: Float, containerSize: Float): Float {
        val margin = containerSize * 0.1f
        return when {
            offset >= 0f && offset + size <= containerSize -> 0f
            size >= containerSize - 2 * margin -> offset - margin
            offset < 0f -> offset - margin
            else -> offset + size - (containerSize - margin)
        }
    }
}

private data class Tab(val route: String, val label: Int, val icon: ImageVector, val outline: ImageVector)

/** Sezioni: icona piena per quella attiva, a contorno per le altre. */
private val TABS = listOf(
    Tab("home", R.string.nav_home, Icons.Filled.Home, Icons.Outlined.Home),
    Tab("search", R.string.nav_search, Icons.Filled.Search, Icons.Outlined.Search),
    Tab("libraries", R.string.nav_libraries, Icons.Filled.VideoLibrary, Icons.Outlined.VideoLibrary),
    Tab("playlists", R.string.nav_playlists, Icons.AutoMirrored.Filled.PlaylistPlay, Icons.AutoMirrored.Outlined.PlaylistPlay),
    Tab("account", R.string.nav_account, Icons.Filled.Person, Icons.Outlined.Person),
)

@Composable
fun Main(start: String = "home") {
    val navController = rememberNavController()
    val context = LocalContext.current
    val nav = remember(navController) { Nav(navController, context) }
    val tv = LocalIsTv.current
    val rail = tv || LocalWidthDp.current >= 600
    val entry by navController.currentBackStackEntryAsState()
    // Nelle pagine interne (scheda, libreria, persona…) resta evidenziata la sezione da cui si è arrivati.
    var current by rememberSaveable { mutableStateOf(start) }
    val route = entry?.destination?.route
    LaunchedEffect(route) { if (route != null && TABS.any { it.route == route }) current = route }

    CompositionLocalProvider(LocalNav provides nav) {
        if (rail) {
            Row(Modifier.fillMaxSize()) {
                val railLine = tokens.line
                NavigationRail(
                    containerColor = tokens.surface1,
                    // Il fondo della barra arriva sotto le barre di sistema; solo il contenuto ne rispetta i margini.
                    windowInsets = WindowInsets.systemBars,
                    modifier = Modifier.fillMaxHeight().drawBehind { drawLine(railLine, Offset(size.width, 0f), Offset(size.width, size.height), 1.dp.toPx()) },
                    header = { Column { Spacer(Modifier.height(12.dp)); Mark(36.dp) } },
                ) {
                    Spacer(Modifier.height(24.dp))
                    TABS.forEach { t ->
                        NavigationRailItem(
                            selected = current == t.route,
                            onClick = { if (current == t.route && route != t.route) navController.popBackStack(t.route, false) else nav.tab(t.route); current = t.route },
                            icon = { Icon(if (current == t.route) t.icon else t.outline, null) },
                            label = { Text(stringResource(t.label), style = androidx.compose.material3.MaterialTheme.typography.labelMedium) },
                            colors = NavigationRailItemDefaults.colors(
                                selectedIconColor = tokens.blueText, selectedTextColor = tokens.ink, indicatorColor = tokens.blueSoft,
                                unselectedIconColor = tokens.inkMuted, unselectedTextColor = tokens.inkMuted,
                            ),
                        )
                    }
                }
                Box(Modifier.weight(1f).fillMaxHeight()) {
                    Routes(navController, start, Modifier)
                    StatusBarScrim()
                }
            }
        } else {
            val lineColor = tokens.line
            Scaffold(
                containerColor = tokens.bg,
                bottomBar = {
                    NavigationBar(
                        containerColor = tokens.bg.copy(alpha = 0.96f), tonalElevation = 0.dp,
                        modifier = Modifier.drawBehind { drawLine(lineColor, Offset(0f, 0f), Offset(size.width, 0f), 1.dp.toPx()) },
                    ) {
                        TABS.forEach { t ->
                            NavigationBarItem(
                                selected = current == t.route,
                                onClick = { if (current == t.route && route != t.route) navController.popBackStack(t.route, false) else nav.tab(t.route); current = t.route },
                                icon = { Icon(if (current == t.route) t.icon else t.outline, null) },
                                label = { Text(stringResource(t.label), maxLines = 1, style = androidx.compose.material3.MaterialTheme.typography.labelMedium) },
                                colors = NavigationBarItemDefaults.colors(
                                    selectedIconColor = tokens.blueText, selectedTextColor = tokens.ink, indicatorColor = tokens.blueSoft,
                                    unselectedIconColor = tokens.inkMuted, unselectedTextColor = tokens.inkMuted,
                                ),
                            )
                        }
                    }
                },
            ) { pad ->
                Box(Modifier.fillMaxSize()) {
                    Routes(navController, start, Modifier.padding(bottom = pad.calculateBottomPadding()))
                    StatusBarScrim()
                }
            }
        }
    }
}

/**
 * Velo sotto la barra di stato: il contenuto scorre da bordo a bordo, ma ora e icone restano leggibili sopra
 * locandine e testo. Sfuma verso il basso, così sugli sfondi delle schede non si vede uno stacco.
 */
@Composable
private fun StatusBarScrim() {
    val bg = tokens.bg
    Box(
        Modifier.fillMaxWidth().windowInsetsTopHeight(WindowInsets.statusBars).background(
            Brush.verticalGradient(0f to bg.copy(alpha = 0.97f), 0.75f to bg.copy(alpha = 0.85f), 1f to bg.copy(alpha = 0f)),
        ),
    )
}

@Composable
private fun Routes(navController: NavHostController, start: String, modifier: Modifier) {
    NavHost(navController, startDestination = start, modifier = modifier.fillMaxSize()) {
        composable("home") { HomeScreen() }
        composable("search") { SearchScreen() }
        composable("libraries") { LibrariesScreen() }
        composable("tv") { app.platea.ui.screens.LiveTvScreen() }
        composable("playlists") { PlaylistsScreen() }
        composable("account") { AccountScreen() }
        composable("about") { AboutScreen() }
        composable("library/{id}") { LibraryScreen(it.arguments?.getString("id") ?: "") }
        composable("item/{id}") { ItemScreen(it.arguments?.getString("id") ?: "") }
        composable("person/{id}") { PersonScreen(it.arguments?.getString("id") ?: "") }
        composable("playlist/{id}") { PlaylistScreen(it.arguments?.getString("id") ?: "") }
        composable(
            "browse?genre={genre}&studio={studio}",
            arguments = listOf(
                navArgument("genre") { type = NavType.StringType; nullable = true; defaultValue = null },
                navArgument("studio") { type = NavType.StringType; nullable = true; defaultValue = null },
            ),
        ) { BrowseScreen(it.arguments?.getString("genre"), it.arguments?.getString("studio")) }
    }
}
