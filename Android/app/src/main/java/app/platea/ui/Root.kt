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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.PlaylistPlay
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.VideoLibrary
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

@Composable
fun Root() {
    val context = LocalContext.current
    val state by Session.state.collectAsState()
    val tv = remember { PlateaApp.isTv(context) }
    val theme = (state as? SessionState.LoggedIn)?.me?.preferences?.theme ?: "auto"
    val width = LocalConfiguration.current.screenWidthDp
    PlateaTheme(theme = theme, forceDark = tv) {
        CompositionLocalProvider(LocalIsTv provides tv, LocalWidthDp provides width) {
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

private data class Tab(val route: String, val label: Int, val icon: ImageVector)

private val TABS = listOf(
    Tab("home", R.string.nav_home, Icons.Filled.Home),
    Tab("search", R.string.nav_search, Icons.Filled.Search),
    Tab("libraries", R.string.nav_libraries, Icons.Filled.VideoLibrary),
    Tab("playlists", R.string.nav_playlists, Icons.AutoMirrored.Filled.PlaylistPlay),
    Tab("account", R.string.nav_account, Icons.Filled.Person),
)

@Composable
fun Main(start: String = "home") {
    val navController = rememberNavController()
    val context = LocalContext.current
    val nav = remember(navController) { Nav(navController, context) }
    val tv = LocalIsTv.current
    val rail = tv || LocalWidthDp.current >= 600
    val entry by navController.currentBackStackEntryAsState()
    val current = entry?.destination?.route

    CompositionLocalProvider(LocalNav provides nav) {
        if (rail) {
            Row(Modifier.fillMaxSize()) {
                NavigationRail(
                    containerColor = tokens.surface1,
                    modifier = Modifier.fillMaxHeight().windowInsetsPadding(WindowInsets.systemBars),
                    header = { Column { Spacer(Modifier.height(12.dp)); Mark(36.dp) } },
                ) {
                    Spacer(Modifier.height(24.dp))
                    TABS.forEach { t ->
                        NavigationRailItem(
                            selected = current == t.route,
                            onClick = { nav.tab(t.route) },
                            icon = { Icon(t.icon, null) },
                            label = { Text(stringResource(t.label)) },
                            colors = NavigationRailItemDefaults.colors(
                                selectedIconColor = tokens.blueText, selectedTextColor = tokens.ink, indicatorColor = tokens.blueSoft,
                                unselectedIconColor = tokens.inkMuted, unselectedTextColor = tokens.inkMuted,
                            ),
                        )
                    }
                }
                Box(Modifier.weight(1f).fillMaxHeight()) { Routes(navController, start, Modifier) }
            }
        } else {
            Scaffold(
                containerColor = tokens.bg,
                bottomBar = {
                    NavigationBar(containerColor = tokens.surface1, tonalElevation = 0.dp) {
                        TABS.forEach { t ->
                            NavigationBarItem(
                                selected = current == t.route,
                                onClick = { nav.tab(t.route) },
                                icon = { Icon(t.icon, null) },
                                label = { Text(stringResource(t.label), maxLines = 1) },
                                colors = NavigationBarItemDefaults.colors(
                                    selectedIconColor = tokens.blueText, selectedTextColor = tokens.ink, indicatorColor = tokens.blueSoft,
                                    unselectedIconColor = tokens.inkMuted, unselectedTextColor = tokens.inkMuted,
                                ),
                            )
                        }
                    }
                },
            ) { pad -> Routes(navController, start, Modifier.padding(bottom = pad.calculateBottomPadding())) }
        }
    }
}

@Composable
private fun Routes(navController: NavHostController, start: String, modifier: Modifier) {
    NavHost(navController, startDestination = start, modifier = modifier.fillMaxSize()) {
        composable("home") { HomeScreen() }
        composable("search") { SearchScreen() }
        composable("libraries") { LibrariesScreen() }
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
