package app.platea.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyGridScope
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.ArrowDropDown
import androidx.compose.material.icons.filled.Movie
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.LiveTv
import androidx.compose.material.icons.filled.Tv
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.platea.R
import app.platea.data.Api
import app.platea.data.GenreList
import app.platea.data.Item
import app.platea.data.ItemPage
import app.platea.data.Library
import app.platea.data.LibraryPage
import app.platea.data.PersonDetail
import app.platea.data.Session
import app.platea.ui.LocalNav
import app.platea.ui.components.EmptyBox
import app.platea.ui.components.ErrorBox
import app.platea.ui.components.LoadingBox
import app.platea.ui.components.LocalIsTv
import app.platea.ui.components.PosterCard
import app.platea.ui.components.ResContent
import app.platea.ui.components.bleed
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.foundation.border
import app.platea.ui.components.focusRing
import app.platea.ui.components.pagePadding
import app.platea.ui.components.pluralRes
import app.platea.ui.components.posterWidth
import app.platea.ui.components.rememberLoad
import app.platea.ui.theme.tokens
import coil.compose.AsyncImage
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter

/** Titolo di pagina con "indietro" facoltativo. */
@Composable
fun PageTitle(title: String, sub: String? = null, back: Boolean = false, inset: Boolean = true) {
    val nav = LocalNav.current
    Row(
        Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.statusBars)
            .padding(horizontal = if (inset) pagePadding() else 0.dp).padding(top = if (back) 8.dp else 16.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (back) {
            // La freccia si allinea al margine: l'area di tocco resta di 48 dp, l'icona sta sul bordo del testo.
            IconButton(onClick = { nav.back() }, modifier = Modifier.offset(x = (-12).dp)) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back), tint = tokens.ink)
            }
        }
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.headlineMedium, color = tokens.ink, maxLines = 2, overflow = TextOverflow.Ellipsis)
            if (sub != null) Text(sub, style = MaterialTheme.typography.bodySmall, color = tokens.inkSubtle)
        }
    }
}

/** Celle della griglia: si adattano alla larghezza dello schermo. */
@Composable
fun gridCells(wide: Boolean = false) = GridCells.Adaptive(minSize = posterWidth(wide))

@Composable
fun gridSpacing() = if (LocalIsTv.current) 24.dp else 12.dp

fun LazyGridScope.fullLine(key: String, content: @Composable () -> Unit) {
    item(key = key, span = { GridItemSpan(maxLineSpan) }) { content() }
}

// --- Librerie ---

@Composable
fun LibrariesScreen() {
    val libs = rememberLoad("libraries") { Api.get<LibraryPage>("/api/v1/libraries").items }
    val tv = rememberLoad("livetv-status") { runCatching { Api.get<app.platea.data.LiveStatus>("/api/v1/livetv/status") }.getOrNull() }.data
    val nav = LocalNav.current
    Column(Modifier.fillMaxSize()) {
        PageTitle(stringResource(R.string.nav_libraries))
        ResContent(libs) { list ->
            if (list.isEmpty() && tv?.enabled != true) {
                EmptyBox(stringResource(if (Session.me?.isAdmin == true) R.string.home_empty_admin else R.string.home_empty))
                return@ResContent
            }
            LazyVerticalGrid(
                columns = GridCells.Adaptive(if (LocalIsTv.current) 360.dp else 320.dp),
                contentPadding = PaddingValues(horizontal = pagePadding(), vertical = 12.dp),
                horizontalArrangement = Arrangement.spacedBy(gridSpacing()),
                verticalArrangement = Arrangement.spacedBy(gridSpacing()),
            ) {
                items(list, key = { it.id }) { lib -> LibraryCard(lib) { nav.library(lib.id) } }
                if (tv?.enabled == true) {
                    item(key = "tv") { TvCard(tv.channels) { nav.tv() } }
                }
            }
        }
    }
}

/** La Live TV accanto alle librerie: stessa card, con l'icona e il numero dei canali. */
@Composable
private fun TvCard(channels: Int, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(14.dp)
    Box(
        Modifier.fillMaxWidth().aspectRatio(16f / 9f).focusRing(interaction, shape, glow = true).clip(shape)
            .background(tokens.surface2)
            .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple(), onClick = onClick),
    ) {
        Icon(Icons.Filled.LiveTv, null, tint = tokens.inkSubtle, modifier = Modifier.align(Alignment.Center).size(56.dp))
        Column(Modifier.align(Alignment.BottomStart).padding(16.dp)) {
            Text(stringResource(R.string.tv_title), style = MaterialTheme.typography.titleLarge, color = tokens.ink)
            Text(androidx.compose.ui.res.pluralStringResource(R.plurals.tv_channels, channels, channels), style = MaterialTheme.typography.bodySmall, color = tokens.inkMuted)
        }
    }
}

/**
 * Libreria: card 16:9 con lo sfondo di un titolo aggiunto di recente (lo stesso finché la libreria non cambia),
 * il nome e il conteggio sopra un velo. Senza immagini resta il fondo neutro con l'icona.
 */
@Composable
private fun LibraryCard(lib: Library, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(14.dp)
    val cover = rememberLoad("library-cover", lib.id) {
        Api.get<ItemPage>("/api/v1/items?library_id=${lib.id}&sort=added&order=desc&limit=8").items.firstOrNull { it.images.backdrop != null }
    }.data
    Box(
        Modifier.fillMaxWidth().aspectRatio(16f / 9f).focusRing(interaction, shape, glow = true).clip(shape)
            .background(tokens.surface2)
            .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple(), onClick = onClick),
    ) {
        cover?.images?.backdrop?.let {
            AsyncImage(Api.image(it, 960), null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
        }
        Box(Modifier.fillMaxSize().background(Brush.verticalGradient(0f to Color.Transparent, 0.45f to Color(0x33000000), 1f to Color(0xD9000000))))
        Box(Modifier.fillMaxSize().border(1.dp, Color.White.copy(alpha = 0.08f), shape))
        Row(Modifier.align(Alignment.BottomStart).padding(18.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(40.dp).clip(RoundedCornerShape(10.dp)).background(Color(0x66000000)).border(1.dp, Color.White.copy(alpha = 0.15f), RoundedCornerShape(10.dp)), contentAlignment = Alignment.Center) {
                Icon(if (lib.kind == "shows") Icons.Filled.Tv else Icons.Filled.Movie, null, tint = Color.White, modifier = Modifier.size(22.dp))
            }
            Spacer(Modifier.width(14.dp))
            Column {
                Text(lib.name, style = MaterialTheme.typography.titleLarge, color = Color.White, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(
                    pluralRes(if (lib.kind == "shows") R.plurals.count_shows else R.plurals.count_movies, lib.itemCount),
                    style = MaterialTheme.typography.bodySmall, color = Color.White.copy(alpha = 0.75f),
                )
            }
        }
    }
}

// --- Elenco paginato ---

/** Elenco a pagine con cursore: carica la pagina successiva quando ci si avvicina al fondo. */
class Paged(private val base: String) {
    val items = mutableStateListOf<Item>()
    var cursor: String? = null
    var done by mutableStateOf(false)
    var loading by mutableStateOf(false)
    var error by mutableStateOf<String?>(null)
    var loadedOnce by mutableStateOf(false)

    suspend fun next(network: String) {
        if (loading || done) return
        loading = true
        try {
            val sep = if ('?' in base) "&" else "?"
            val p = Api.get<ItemPage>(base + (cursor?.let { "${sep}cursor=${Api.enc(it)}" } ?: ""))
            items.addAll(p.items.filter { n -> items.none { it.id == n.id } })
            cursor = p.nextCursor
            done = p.nextCursor == null
            error = null
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: Exception) {
            error = Api.message(e, network)
        } finally {
            loading = false
            loadedOnce = true
        }
    }
}

/** Griglia di locandine paginata, con intestazione libera. */
@Composable
fun PagedGrid(url: String, header: (LazyGridScope.() -> Unit)? = null, empty: String) {
    val nav = LocalNav.current
    val network = stringResource(R.string.error_network)
    val tick by Session.catalogTick.collectAsState()
    val paged = remember(url, tick) { Paged(url) }
    val state = rememberLazyGridState()
    LaunchedEffect(paged) { paged.next(network) }
    val nearEnd by remember(paged) {
        derivedStateOf {
            val last = state.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0
            last >= state.layoutInfo.totalItemsCount - 12
        }
    }
    LaunchedEffect(paged) {
        snapshotFlow { nearEnd }.distinctUntilChanged().filter { it }.collect { paged.next(network) }
    }
    LazyVerticalGrid(
        state = state,
        columns = gridCells(),
        contentPadding = PaddingValues(start = pagePadding(), end = pagePadding(), bottom = 32.dp),
        horizontalArrangement = Arrangement.spacedBy(gridSpacing()),
        verticalArrangement = Arrangement.spacedBy(if (LocalIsTv.current) 24.dp else 16.dp),
        modifier = Modifier.fillMaxSize(),
    ) {
        header?.invoke(this)
        items(paged.items, key = { it.id }) { item ->
            PosterCard(item, width = null, modifier = Modifier.fillMaxWidth()) { nav.item(item.id) }
        }
        when {
            paged.error != null && paged.items.isEmpty() -> fullLine("err") { ErrorBox(paged.error!!) }
            paged.loadedOnce && paged.items.isEmpty() && !paged.loading -> fullLine("empty") { EmptyBox(empty) }
            paged.loading -> fullLine("loading") { LoadingBox() }
        }
    }
}


// --- Libreria ---

private val LETTERS = "#ABCDEFGHIJKLMNOPQRSTUVWXYZ".map { it.toString() }

@Composable
fun LibraryScreen(id: String) {
    val lib = rememberLoad("library", id) { Api.get<Library>("/api/v1/libraries/$id") }.data
    val genres = rememberLoad("genres", id) { Api.get<GenreList>("/api/v1/genres?library_id=$id").items }.data ?: emptyList()
    var sort by rememberSaveable { mutableStateOf("title") }
    var filter by rememberSaveable { mutableStateOf("all") }
    var genre by rememberSaveable { mutableStateOf("") }
    var decade by rememberSaveable { mutableStateOf("") }
    var video by rememberSaveable { mutableStateOf("") }
    var letter by rememberSaveable { mutableStateOf("") }
    var text by rememberSaveable { mutableStateOf("") }
    var query by rememberSaveable { mutableStateOf("") }
    LaunchedEffect(text) { delay(300); query = text.trim() }

    val params = buildList {
        add("library_id=$id"); add("sort=$sort"); add("order=${if (sort == "title") "asc" else "desc"}"); add("limit=60")
        when (filter) {
            "unplayed" -> add("played=false"); "played" -> add("played=true"); "favorite" -> add("favorite=true"); "collections" -> add("kind=collection")
        }
        if (genre.isNotEmpty()) add("genre=${Api.enc(genre)}")
        if (decade.isNotEmpty()) add("decade=$decade")
        if (video.isNotEmpty()) add("video=$video")
        if (query.isNotEmpty()) add("q=${Api.enc(query)}") else if (letter.isNotEmpty() && sort == "title") add("letter=${Api.enc(letter)}")
    }.joinToString("&")

    val all = stringResource(R.string.filter_all)
    PagedGrid(
        url = "/api/v1/items?$params",
        empty = if (query.isNotEmpty()) stringResource(R.string.search_none, query) else stringResource(R.string.library_empty),
        header = {
            fullLine("title") {
                PageTitle(
                    lib?.name ?: "",
                    lib?.let { pluralRes(if (it.kind == "shows") R.plurals.count_shows else R.plurals.count_movies, it.itemCount) },
                    back = true,
                    inset = false,
                )
            }
            fullLine("filters") {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.padding(bottom = 8.dp)) {
                    SearchField(text, { text = it }, stringResource(R.string.library_search_in, lib?.name ?: ""))
                    val pad = pagePadding()
                    LazyRow(Modifier.bleed(pad), contentPadding = PaddingValues(horizontal = pad), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        item {
                            Dropdown(stringResource(R.string.sort), sort, listOf(
                                "title" to stringResource(R.string.sort_title), "added" to stringResource(R.string.sort_added),
                                "year" to stringResource(R.string.sort_year), "rating" to stringResource(R.string.sort_rating),
                                "played" to stringResource(R.string.sort_played),
                            ), highlight = sort != "title") { sort = it }
                        }
                        item {
                            Dropdown(stringResource(R.string.filter), filter, buildList {
                                add("all" to all); add("unplayed" to stringResource(R.string.filter_unplayed))
                                add("played" to stringResource(R.string.filter_played)); add("favorite" to stringResource(R.string.filter_favorite))
                                if (lib?.kind == "movies") add("collections" to stringResource(R.string.filter_collections))
                            }, highlight = filter != "all") { filter = it }
                        }
                        item {
                            Dropdown(stringResource(R.string.decade), decade, listOf("" to all) +
                                listOf(2020, 2010, 2000, 1990, 1980, 1970, 1960, 1950).map { "$it" to "${it}s" } +
                                listOf("1940" to stringResource(R.string.decade_old)), highlight = decade.isNotEmpty()) { decade = it }
                        }
                        item {
                            Dropdown(stringResource(R.string.video), video, listOf("" to all, "4k" to "4K", "1080" to "1080p", "720" to "720p", "sd" to "SD", "hdr" to "HDR"), highlight = video.isNotEmpty()) { video = it }
                        }
                        if (genres.isNotEmpty()) item {
                            Dropdown(stringResource(R.string.genre), genre, listOf("" to all) + genres.map { it.name to it.name }, highlight = genre.isNotEmpty()) { genre = it }
                        }
                    }
                    if (sort == "title" && query.isEmpty()) {
                        LazyRow(Modifier.bleed(pad), contentPadding = PaddingValues(horizontal = pad), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            items(LETTERS) { l -> LetterChip(l, l == letter) { letter = if (letter == l) "" else l } }
                        }
                    }
                }
            }
        },
    )
}

@Composable
fun SearchField(value: String, onChange: (String) -> Unit, label: String, focus: FocusRequester? = null) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        leadingIcon = { Icon(Icons.Filled.Search, null, tint = tokens.inkMuted) },
        singleLine = true,
        colors = plateaFieldColors(), shape = app.platea.ui.theme.Radius.md,
        modifier = Modifier.fillMaxWidth().then(if (focus != null) Modifier.focusRequester(focus) else Modifier),
    )
}

/** Scelta a tendina compatta (ordine, filtri). */
@Composable
fun Dropdown(label: String, value: String, options: List<Pair<String, String>>, highlight: Boolean = false, onChange: (String) -> Unit) {
    var open by remember { mutableStateOf(false) }
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(10.dp)
    val current = options.firstOrNull { it.first == value }?.second ?: options.firstOrNull()?.second ?: ""
    Box {
        Surface(
            color = if (highlight) tokens.blueSoft else tokens.surface2,
            shape = shape,
            modifier = Modifier.focusRing(interaction, shape, grow = false).clip(shape)
                .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple()) { open = true },
        ) {
            Row(Modifier.padding(start = 12.dp, end = 6.dp, top = 8.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("$label: ", style = MaterialTheme.typography.labelMedium, color = tokens.inkSubtle)
                Text(current, style = MaterialTheme.typography.labelLarge, color = tokens.ink)
                Icon(Icons.Filled.ArrowDropDown, null, tint = tokens.inkMuted)
            }
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }, containerColor = tokens.surface2) {
            options.forEach { (v, l) ->
                DropdownMenuItem(
                    text = { Text(l, color = if (v == value) tokens.blueText else tokens.ink, fontWeight = if (v == value) FontWeight.SemiBold else FontWeight.Normal) },
                    onClick = { open = false; onChange(v) },
                )
            }
        }
    }
}

@Composable
private fun LetterChip(l: String, active: Boolean, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    Box(
        Modifier.size(36.dp).focusRing(interaction, CircleShape).clip(CircleShape)
            .background(if (active) tokens.blue else tokens.surface2)
            .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple(), onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(l, style = MaterialTheme.typography.labelLarge, color = if (active) androidx.compose.ui.graphics.Color.White else tokens.inkMuted, textAlign = TextAlign.Center)
    }
}

// --- Genere / studio ---

@Composable
fun BrowseScreen(genre: String?, studio: String?) {
    val params = buildList {
        add("sort=title"); add("order=asc"); add("limit=60")
        genre?.let { add("genre=${Api.enc(it)}") }
        studio?.let { add("studio=${Api.enc(it)}") }
    }.joinToString("&")
    PagedGrid(
        url = "/api/v1/items?$params",
        empty = stringResource(R.string.library_empty),
        header = { fullLine("title") { PageTitle(genre ?: studio ?: "", stringResource(if (genre != null) R.string.genre else R.string.studio), back = true, inset = false) } },
    )
}

// --- Ricerca ---

@Composable
fun SearchScreen() {
    var text by rememberSaveable { mutableStateOf("") }
    var query by rememberSaveable { mutableStateOf("") }
    LaunchedEffect(text) { delay(250); query = text.trim() }
    val libs = rememberLoad("libraries") { Api.get<LibraryPage>("/api/v1/libraries").items }.data ?: emptyList()
    val results = rememberLoad("search", query) { if (query.isEmpty()) emptyList() else Api.get<ItemPage>("/api/v1/search?q=${Api.enc(query)}&limit=100").items }
    val nav = LocalNav.current
    val focus = remember { FocusRequester() }
    val tv = LocalIsTv.current
    LaunchedEffect(Unit) { if (!tv) runCatching { focus.requestFocus() } }
    LazyVerticalGrid(
        columns = gridCells(),
        contentPadding = PaddingValues(start = pagePadding(), end = pagePadding(), bottom = 32.dp),
        horizontalArrangement = Arrangement.spacedBy(gridSpacing()),
        verticalArrangement = Arrangement.spacedBy(16.dp),
        modifier = Modifier.fillMaxSize(),
    ) {
        fullLine("title") { PageTitle(stringResource(R.string.nav_search), inset = false) }
        fullLine("field") { SearchField(text, { text = it }, stringResource(R.string.search_label), focus) }
        val list = results.data ?: emptyList()
        if (query.isEmpty()) {
            fullLine("hint") { EmptyBox(stringResource(R.string.search_hint)) }
        }
        if (query.isNotEmpty() && results.data != null && list.isEmpty()) {
            fullLine("none") { EmptyBox(stringResource(R.string.search_none, query)) }
        }
        // Un gruppo per libreria: lo stesso film in due librerie si distingue.
        val order = libs.withIndex().associate { it.value.id to it.index }
        val groups = list.groupBy { it.libraryId }.toList().sortedBy { order[it.first] ?: 999 }
        for ((libId, items) in groups) {
            if (groups.size > 1) {
                fullLine("h-$libId") {
                    Text(
                        libs.firstOrNull { it.id == libId }?.name ?: "",
                        style = MaterialTheme.typography.titleLarge, color = tokens.ink, modifier = Modifier.padding(top = 12.dp),
                    )
                }
            }
            items(items, key = { "$libId-${it.id}" }) { item -> PosterCard(item, width = null, modifier = Modifier.fillMaxWidth()) { nav.item(item.id) } }
        }
    }
}

// --- Persona ---

@Composable
fun PersonScreen(id: String) {
    val person = rememberLoad("person", id) { Api.get<PersonDetail>("/api/v1/people/$id") }
    val libs = rememberLoad("libraries") { Api.get<LibraryPage>("/api/v1/libraries").items }.data ?: emptyList()
    val nav = LocalNav.current
    ResContent(person) { p ->
        val order = libs.withIndex().associate { it.value.id to it.index }
        val groups = p.items.groupBy { it.libraryId }.toList().sortedBy { order[it.first] ?: 999 }
        LazyVerticalGrid(
            columns = gridCells(),
            contentPadding = PaddingValues(start = pagePadding(), end = pagePadding(), bottom = 32.dp),
            horizontalArrangement = Arrangement.spacedBy(gridSpacing()),
            verticalArrangement = Arrangement.spacedBy(16.dp),
            modifier = Modifier.fillMaxSize(),
        ) {
            fullLine("head") {
                Column {
                    PageTitle(p.name, back = true, inset = false)
                    if (p.image != null) {
                        AsyncImage(
                            model = Api.image(p.image, 300), contentDescription = p.name, contentScale = ContentScale.Crop,
                            modifier = Modifier.padding(vertical = 8.dp).width(140.dp).aspectRatio(2f / 3f).clip(RoundedCornerShape(12.dp)).background(tokens.placeholder),
                        )
                    }
                }
            }
            for ((libId, items) in groups) {
                if (groups.size > 1) {
                    fullLine("h-$libId") {
                        Text(libs.firstOrNull { it.id == libId }?.name ?: "", style = MaterialTheme.typography.titleLarge, color = tokens.ink, modifier = Modifier.padding(top = 12.dp))
                    }
                }
                items(items, key = { "$libId-${it.id}" }) { item ->
                    val role = item.roles.firstOrNull()
                    val meta = listOfNotNull(item.year?.toString(), role?.character ?: role?.role?.let { roleLabel(it) }).joinToString(" · ")
                    PosterCard(item, width = null, meta = meta, modifier = Modifier.fillMaxWidth()) { nav.item(item.id) }
                }
            }
        }
    }
}

@Composable
fun roleLabel(role: String): String = when (role) {
    "director" -> stringResource(R.string.role_director)
    "writer" -> stringResource(R.string.role_writer)
    "producer" -> stringResource(R.string.role_producer)
    "composer" -> stringResource(R.string.role_composer)
    "creator" -> stringResource(R.string.role_creator)
    "actor" -> stringResource(R.string.role_actor)
    else -> role
}

