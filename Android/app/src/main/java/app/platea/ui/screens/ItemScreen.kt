package app.platea.ui.screens

import android.app.DownloadManager
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Environment
import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.PlaylistAdd
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Favorite
import androidx.compose.material.icons.filled.FavoriteBorder
import androidx.compose.material.icons.filled.Movie
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.platea.R
import app.platea.data.Api
import app.platea.data.DownloadLink
import app.platea.data.Item
import app.platea.data.ItemDetail
import app.platea.data.ItemPage
import app.platea.data.MediaFile
import app.platea.data.MissingEpisode
import app.platea.data.MissingList
import app.platea.data.PlaylistList
import app.platea.data.Session
import app.platea.ui.LocalNav
import app.platea.ui.components.Badge
import app.platea.ui.components.LocalIsTv
import app.platea.ui.components.LocalWidthDp
import app.platea.ui.components.PosterCard
import app.platea.ui.components.PosterRow
import app.platea.ui.components.PrimaryButton
import app.platea.ui.components.ProgressLine
import app.platea.ui.components.ResContent
import app.platea.ui.components.RoundIconButton
import app.platea.ui.components.SecondaryButton
import app.platea.ui.components.SectionHeader
import app.platea.ui.components.WatchedBadge
import app.platea.ui.components.focusRing
import app.platea.ui.components.pagePadding
import app.platea.ui.components.parseColor
import app.platea.ui.components.pluralRes
import app.platea.ui.components.rememberLoad
import app.platea.ui.theme.tokens
import app.platea.util.Fmt
import coil.compose.AsyncImage
import kotlinx.coroutines.launch

@Composable
fun ItemScreen(id: String) {
    val loader = rememberLoad("item", id) { Api.get<ItemDetail>("/api/v1/items/$id") }
    ResContent(loader) { item -> ItemContent(item, onChanged = loader.reload) }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ItemContent(item: ItemDetail, onChanged: () -> Unit) {
    val nav = LocalNav.current
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val tv = LocalIsTv.current
    val wide = LocalWidthDp.current >= 600
    val me = Session.me
    val expired = me?.permissions?.expired == true

    val seriesId = when (item.kind) { "series" -> item.id; "season" -> item.parentId; else -> null }
    val seasons = rememberLoad("seasons", seriesId) { if (seriesId == null) emptyList() else Api.get<ItemPage>("/api/v1/items/$seriesId/children").items }.data ?: emptyList()
    var season by rememberSaveable(item.id) { mutableStateOf<String?>(null) }
    val currentSeason = season ?: if (item.kind == "season") item.id else
        (seasons.firstOrNull { (it.unplayedCount ?: 0) > 0 && (it.indexNumber ?: 0) > 0 } ?: seasons.firstOrNull())?.id
    val episodes = rememberLoad("episodes", currentSeason) { if (currentSeason == null) null else Api.get<ItemPage>("/api/v1/items/$currentSeason/children").items }.data
    val missing = rememberLoad("missing", currentSeason) { if (currentSeason == null) emptyList() else runCatching { Api.get<MissingList>("/api/v1/items/$currentSeason/missing").items }.getOrDefault(emptyList()) }.data ?: emptyList()
    val similarFor = if (item.kind == "movie" || item.kind == "series") item.id else null
    val similar = rememberLoad("similar", similarFor) { if (similarFor == null) emptyList() else Api.get<ItemPage>("/api/v1/items/$similarFor/similar").items }.data ?: emptyList()
    val members = rememberLoad("members", item.id, item.kind) { if (item.kind != "collection") emptyList() else Api.get<ItemPage>("/api/v1/items/${item.id}/children").items }.data ?: emptyList()

    var fileIdx by rememberSaveable(item.id) { mutableIntStateOf(0) }
    val file = item.files.getOrNull(fileIdx)
    var dialog by remember { mutableStateOf<String?>(null) }
    val nextEpisode = episodes?.firstOrNull { !it.userData.played } ?: episodes?.firstOrNull()
    val resume = item.isPlayable && item.userData.positionMs > 0 && !item.userData.played
    val playFocus = remember { FocusRequester() }
    LaunchedEffect(item.id) { if (tv) runCatching { playFocus.requestFocus() } }

    fun toggle(what: String, on: Boolean) = scope.launch {
        runCatching {
            if (on) Api.postUnit("/api/v1/items/${item.id}/$what") else Api.delete("/api/v1/items/${item.id}/$what")
        }.onFailure { Toast.makeText(context, Api.message(it, context.getString(R.string.error_network)), Toast.LENGTH_SHORT).show() }
        onChanged()
        Session.bumpCatalog()
    }

    val meta = listOfNotNull(
        if (item.kind == "episode") "S${item.parentIndexNumber} · E${item.indexNumber}" else null,
        item.year?.toString(),
        item.runtimeMs?.let { Fmt.duration(it) },
        item.officialRating,
        item.communityRating?.let { Fmt.rating(it) },
        if (item.kind == "series" && seasons.isNotEmpty()) pluralRes(R.plurals.seasons, seasons.count { (it.indexNumber ?: 0) > 0 }) else null,
        if (item.kind == "collection" && members.isNotEmpty()) pluralRes(R.plurals.titles, members.size) else null,
    )

    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = 40.dp)) {
        item("header") {
            Box(Modifier.fillMaxWidth()) {
                val backdrop = item.images.backdrop ?: item.images.still
                Box(Modifier.fillMaxWidth().height(if (tv) 280.dp else if (wide) 360.dp else 260.dp)) {
                    if (backdrop != null) {
                        AsyncImage(Api.image(backdrop, 1280), null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
                    }
                    Box(Modifier.fillMaxSize().background(Brush.verticalGradient(0f to tokens.bg.copy(alpha = 0.2f), 1f to tokens.bg)))
                }
                IconButton(onClick = { nav.back() }, modifier = Modifier.windowInsetsPadding(WindowInsets.statusBars).padding(8.dp).background(tokens.scrim, CircleShape)) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back), tint = Color.White)
                }
                Row(
                    Modifier.padding(horizontal = pagePadding()).padding(top = if (tv) 96.dp else if (wide) 220.dp else 150.dp),
                    verticalAlignment = Alignment.Bottom,
                ) {
                    if (item.kind != "episode") {
                        Box(
                            Modifier.width(if (tv) 140.dp else if (wide) 180.dp else 110.dp).aspectRatio(2f / 3f).clip(RoundedCornerShape(12.dp))
                                .background(parseColor(item.images.posterColor) ?: tokens.placeholder),
                        ) {
                            item.images.poster?.let { AsyncImage(Api.image(it, 400), null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize()) }
                        }
                        Spacer(Modifier.width(16.dp))
                    }
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        if (item.seriesTitle != null && item.seriesId != null) {
                            Text(item.seriesTitle, style = MaterialTheme.typography.labelLarge, color = tokens.blueText, modifier = Modifier.clickable { nav.item(item.seriesId) })
                        }
                        val logo = item.images.logo
                        if (logo != null && item.kind != "episode") {
                            AsyncImage(Api.image(logo, 600), item.title, contentScale = ContentScale.Fit, alignment = Alignment.CenterStart, modifier = Modifier.heightIn(max = 80.dp).widthIn(max = 320.dp))
                        } else {
                            Text(item.title, style = MaterialTheme.typography.headlineMedium, color = tokens.ink, maxLines = 3, overflow = TextOverflow.Ellipsis)
                        }
                        if (meta.isNotEmpty()) Text(meta.joinToString("  ·  "), style = MaterialTheme.typography.bodySmall, color = tokens.inkMuted)
                    }
                }
            }
        }

        item("info") {
            Column(Modifier.padding(horizontal = pagePadding()).padding(top = 16.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                if (expired) {
                    Surface(color = tokens.warning.copy(alpha = 0.12f), shape = RoundedCornerShape(10.dp)) {
                        Text(stringResource(R.string.item_expired), color = tokens.warning, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.padding(12.dp))
                    }
                }
                // Azioni: riproduzione, trailer, visto, preferito, playlist, download.
                FlowRow(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    if (!expired && (item.isPlayable || nextEpisode != null)) {
                        val label = when {
                            resume -> stringResource(R.string.item_resume_from, Fmt.duration(item.userData.positionMs))
                            item.isPlayable -> stringResource(R.string.play)
                            nextEpisode != null -> stringResource(R.string.play) + " " + nextEpisode.episodeCode
                            else -> ""
                        }
                        PrimaryButton(label, Icons.Filled.PlayArrow, modifier = Modifier.focusRequester(playFocus)) {
                            if (item.isPlayable) nav.play(item.id, file?.id) else nextEpisode?.let { nav.play(it.id) }
                        }
                        if (resume) {
                            SecondaryButton(stringResource(R.string.item_from_start)) { nav.play(item.id, file?.id, startOver = true) }
                        }
                    }
                    if (item.trailers.isNotEmpty()) {
                        SecondaryButton(stringResource(R.string.item_trailer), Icons.Filled.Movie) { openTrailer(context, item.trailers.first().key) }
                    }
                    if (item.kind != "collection") {
                        RoundIconButton(Icons.Filled.Check, stringResource(if (item.userData.played) R.string.mark_unplayed else R.string.mark_played), item.userData.played) {
                            toggle("played", !item.userData.played)
                        }
                        RoundIconButton(if (item.userData.favorite) Icons.Filled.Favorite else Icons.Filled.FavoriteBorder, stringResource(R.string.favorite), item.userData.favorite) {
                            toggle("favorite", !item.userData.favorite)
                        }
                    }
                    RoundIconButton(Icons.AutoMirrored.Filled.PlaylistAdd, stringResource(R.string.playlist_add_to)) { dialog = "playlist" }
                    if (item.isPlayable && me?.permissions?.allowDownload == true && !expired && !tv) {
                        RoundIconButton(Icons.Filled.Download, stringResource(R.string.item_download)) {
                            scope.launch { download(context, item.id, file?.id) }
                        }
                    }
                }
                if (resume && item.runtimeMs != null) Box(Modifier.widthIn(max = 360.dp)) { ProgressLine(item.userData.positionMs.toFloat() / item.runtimeMs) }
                item.tagline?.takeIf { it.isNotBlank() }?.let { Text(it, style = MaterialTheme.typography.titleSmall, color = tokens.inkMuted) }
                item.overview?.takeIf { it.isNotBlank() }?.let { Text(it, style = MaterialTheme.typography.bodyLarge, color = tokens.inkMuted) }
                if (item.genres.isNotEmpty()) {
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        item.genres.forEach { g -> Chip(g) { nav.genre(g) } }
                    }
                }
                if (item.studios.isNotEmpty() && item.kind != "episode") {
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(stringResource(R.string.studios), style = MaterialTheme.typography.bodySmall, color = tokens.inkSubtle)
                        item.studios.take(4).forEach { s -> Chip(s) { nav.studio(s) } }
                    }
                }
                if (item.collections.isNotEmpty()) {
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(stringResource(R.string.part_of), style = MaterialTheme.typography.bodySmall, color = tokens.inkSubtle)
                        item.collections.forEach { c -> Chip(c.title) { nav.item(c.id) } }
                    }
                }
            }
        }

        if (members.isNotEmpty()) {
            item("members") {
                SectionHeader(stringResource(R.string.collection_titles))
                PosterRow(members) { nav.item(it.id) }
            }
        }

        if (item.extras.isNotEmpty()) {
            item("extras") {
                SectionHeader(stringResource(R.string.extras))
                LazyRow(contentPadding = PaddingValues(horizontal = pagePadding()), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    items(item.extras, key = { it.fileId }) { x ->
                        ExtraCard(x.title, listOfNotNull(x.kind?.let { extraKind(it) }, x.durationMs?.let { Fmt.duration(it) }).joinToString(" · ")) {
                            nav.play(item.id, x.fileId)
                        }
                    }
                }
            }
        }

        if (seasons.isNotEmpty()) {
            item("seasons") {
                LazyRow(
                    contentPadding = PaddingValues(horizontal = pagePadding(), vertical = 16.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    items(seasons, key = { it.id }) { s -> SeasonTab(s.title, s.id == currentSeason) { season = s.id } }
                }
            }
            val rows: List<Any> = ((episodes ?: emptyList()) + missing).sortedBy { if (it is Item) it.indexNumber ?: 0 else (it as MissingEpisode).episode }
            items(rows.size, key = { i -> rows[i].let { if (it is Item) it.id else "m${(it as MissingEpisode).season}-${it.episode}" } }) { i ->
                when (val r = rows[i]) {
                    is Item -> EpisodeRow(r, onOpen = { nav.item(r.id) }, onPlay = { nav.play(r.id) })
                    is MissingEpisode -> MissingRow(r)
                }
            }
        }

        if (file != null) {
            item("tech") {
                SectionHeader(stringResource(R.string.technical))
                Column(Modifier.padding(horizontal = pagePadding()), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    if (item.files.size > 1) {
                        Dropdown(stringResource(R.string.version), fileIdx.toString(), options = item.files.mapIndexed { i, f -> i.toString() to (f.versionLabel ?: f.name) }) { fileIdx = it.toInt() }
                    }
                    TechInfo(file)
                }
            }
        }

        if (item.people.isNotEmpty()) {
            item("cast") {
                SectionHeader(stringResource(R.string.cast))
                LazyRow(contentPadding = PaddingValues(horizontal = pagePadding()), horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    items(item.people.size) { i ->
                        val p = item.people[i]
                        PersonCard(p.name, if (p.role == "actor") p.character else roleLabel(p.role), p.image) { nav.person(p.id) }
                    }
                }
            }
        }

        if (similar.isNotEmpty()) {
            item("similar") {
                SectionHeader(stringResource(R.string.similar))
                PosterRow(similar) { nav.item(it.id) }
            }
        }
    }

    if (dialog == "playlist") AddToPlaylistDialog(item.id) { dialog = null }
}

/** Trailer su YouTube: app di YouTube se c'è (anche sulla TV), altrimenti il browser. */
private fun openTrailer(context: Context, key: String) {
    val app = Intent(Intent.ACTION_VIEW, Uri.parse("vnd.youtube:$key"))
    val web = Intent(Intent.ACTION_VIEW, Uri.parse("https://www.youtube.com/watch?v=$key"))
    try { context.startActivity(app) } catch (_: ActivityNotFoundException) {
        try { context.startActivity(web) } catch (_: ActivityNotFoundException) {
            Toast.makeText(context, R.string.no_app_for_trailer, Toast.LENGTH_SHORT).show()
        }
    }
}

/** File originale con il gestore dei download di Android: link firmato del server, riprendibile. */
private suspend fun download(context: Context, itemId: String, fileId: String?) {
    try {
        val r = Api.post<DownloadLink>("/api/v1/items/$itemId/download", DownloadBody(fileId))
        val dm = context.getSystemService(Context.DOWNLOAD_SERVICE) as DownloadManager
        val req = DownloadManager.Request(Uri.parse(Api.url(r.url)))
            .setTitle(r.name)
            .setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
            .setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, r.name.replace('/', '_'))
        dm.enqueue(req)
        Toast.makeText(context, context.getString(R.string.download_started, r.name), Toast.LENGTH_LONG).show()
    } catch (e: Exception) {
        Toast.makeText(context, Api.message(e, context.getString(R.string.error_network)), Toast.LENGTH_LONG).show()
    }
}

@kotlinx.serialization.Serializable
data class DownloadBody(@kotlinx.serialization.SerialName("file_id") val fileId: String?)

@Composable
fun Chip(text: String, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(8.dp)
    Surface(
        color = tokens.surface2, shape = shape,
        modifier = Modifier.focusRing(interaction, shape, grow = false).clip(shape)
            .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple(), onClick = onClick),
    ) {
        Text(text, style = MaterialTheme.typography.labelLarge, color = tokens.ink, modifier = Modifier.padding(horizontal = 12.dp, vertical = 7.dp))
    }
}

@Composable
private fun SeasonTab(title: String, active: Boolean, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(999.dp)
    Surface(
        color = if (active) tokens.ink else tokens.surface2, shape = shape,
        modifier = Modifier.focusRing(interaction, shape, grow = false).clip(shape)
            .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple(), onClick = onClick),
    ) {
        Text(title, style = MaterialTheme.typography.labelLarge, color = if (active) tokens.bg else tokens.ink, modifier = Modifier.padding(horizontal = 16.dp, vertical = 9.dp))
    }
}

@Composable
private fun EpisodeRow(e: Item, onOpen: () -> Unit, onPlay: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(12.dp)
    val wide = LocalWidthDp.current >= 600
    Row(
        Modifier.fillMaxWidth().padding(horizontal = pagePadding(), vertical = 6.dp)
            .focusRing(interaction, shape, grow = false).clip(shape)
            .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple(), onClick = onOpen)
            .padding(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width(if (wide) 200.dp else 140.dp).aspectRatio(16f / 9f).clip(RoundedCornerShape(8.dp)).background(tokens.placeholder)) {
            e.images.still?.let { AsyncImage(Api.image(it, 360), null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize()) }
            if (e.progress > 0f) ProgressLine(e.progress, Modifier.align(Alignment.BottomStart))
            if (e.userData.played) WatchedBadge(Modifier.align(Alignment.TopEnd).padding(4.dp))
        }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text("${e.indexNumber ?: ""}. ${e.title}", style = MaterialTheme.typography.titleSmall, color = tokens.ink, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(
                listOfNotNull(e.premiereDate?.let { Fmt.isoDate(it) }, e.runtimeMs?.let { Fmt.duration(it) }).joinToString(" · "),
                style = MaterialTheme.typography.bodySmall, color = tokens.inkSubtle,
            )
            e.overview?.takeIf { it.isNotBlank() }?.let {
                Text(it, style = MaterialTheme.typography.bodySmall, color = tokens.inkMuted, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
        }
        if (Session.me?.permissions?.expired != true) {
            IconButton(onClick = onPlay) { Icon(Icons.Filled.PlayArrow, stringResource(R.string.play), tint = tokens.ink) }
        }
    }
}

@Composable
private fun MissingRow(m: MissingEpisode) {
    Row(Modifier.fillMaxWidth().padding(horizontal = pagePadding(), vertical = 6.dp).padding(6.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.width(if (LocalWidthDp.current >= 600) 200.dp else 140.dp).aspectRatio(16f / 9f).clip(RoundedCornerShape(8.dp)).background(tokens.surface1))
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("${m.episode}. ${m.title ?: stringResource(R.string.episode_n, m.episode)}", style = MaterialTheme.typography.titleSmall, color = tokens.inkSubtle, modifier = Modifier.weight(1f, fill = false))
                Spacer(Modifier.width(8.dp))
                Badge(stringResource(R.string.missing))
            }
            m.airDate?.let { Text(Fmt.isoDate(it), style = MaterialTheme.typography.bodySmall, color = tokens.inkSubtle) }
        }
    }
}

@Composable
private fun ExtraCard(title: String, sub: String, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(12.dp)
    Surface(
        color = tokens.surface2, shape = shape,
        modifier = Modifier.width(240.dp).focusRing(interaction, shape).clip(shape)
            .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple(), onClick = onClick),
    ) {
        Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(36.dp).clip(CircleShape).background(tokens.blue), contentAlignment = Alignment.Center) {
                Icon(Icons.Filled.PlayArrow, null, tint = Color.White, modifier = Modifier.size(20.dp))
            }
            Spacer(Modifier.width(10.dp))
            Column {
                Text(title, style = MaterialTheme.typography.labelLarge, color = tokens.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (sub.isNotEmpty()) Text(sub, style = MaterialTheme.typography.bodySmall, color = tokens.inkSubtle)
            }
        }
    }
}

@Composable
private fun PersonCard(name: String, sub: String?, image: String?, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    Column(
        Modifier.width(if (LocalIsTv.current) 120.dp else 96.dp).clickable(interactionSource = interaction, indication = null, onClick = onClick),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(Modifier.fillMaxWidth().aspectRatio(1f).focusRing(interaction, CircleShape).clip(CircleShape).background(tokens.placeholder), contentAlignment = Alignment.Center) {
            if (image != null) AsyncImage(Api.image(image, 200), null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
            else Text(name.split(' ').mapNotNull { it.firstOrNull()?.uppercase() }.take(2).joinToString(""), color = tokens.inkMuted, style = MaterialTheme.typography.titleMedium)
        }
        Spacer(Modifier.height(6.dp))
        Text(name, style = MaterialTheme.typography.labelMedium, color = tokens.ink, maxLines = 2, overflow = TextOverflow.Ellipsis)
        if (!sub.isNullOrBlank()) Text(sub, style = MaterialTheme.typography.bodySmall, color = tokens.inkSubtle, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

@Composable
private fun TechInfo(f: MediaFile) {
    val rows = buildList {
        add(stringResource(R.string.tech_file) to listOfNotNull(f.container?.uppercase(), Fmt.size(f.size), f.bitrate?.let { Fmt.bitrate(it) }).joinToString(" · "))
        f.video.forEach { v ->
            add(stringResource(R.string.tech_video) to listOfNotNull(
                v.codec.uppercase(), v.profile, if (v.width != null && v.height != null) "${v.width}×${v.height}" else null,
                v.bitDepth?.let { "$it bit" }, v.hdr?.uppercase(), v.frameRate?.let { "%.3g fps".format(it) },
            ).joinToString(" · "))
        }
        f.audio.forEach { a ->
            add(stringResource(R.string.tech_audio) to listOfNotNull(a.language?.let { Fmt.language(it) }, a.codec.uppercase(), a.channels?.let { Fmt.channels(it) }, a.title).joinToString(" · "))
        }
        f.subtitles.forEach { s ->
            add(stringResource(R.string.tech_subtitles) to listOfNotNull(
                s.language?.let { Fmt.language(it) }, s.codec.replace("hdmv_pgs_subtitle", "pgs").uppercase(), s.title,
                if (s.forced) stringResource(R.string.forced) else null, if (s.external) stringResource(R.string.external) else null,
            ).joinToString(" · "))
        }
    }
    Surface(color = tokens.surface1, shape = RoundedCornerShape(12.dp), modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(f.name, style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace, color = tokens.inkSubtle)
            rows.forEach { (k, v) ->
                Row {
                    Text(k, style = MaterialTheme.typography.labelMedium, color = tokens.inkSubtle, modifier = Modifier.width(92.dp))
                    Text(v, style = MaterialTheme.typography.bodySmall, color = tokens.ink)
                }
            }
        }
    }
}

@Composable
fun extraKind(kind: String): String = when (kind) {
    "trailer" -> stringResource(R.string.extra_trailer)
    "featurette" -> stringResource(R.string.extra_featurette)
    "behindthescenes" -> stringResource(R.string.extra_behind)
    "deleted" -> stringResource(R.string.extra_deleted)
    "interview" -> stringResource(R.string.extra_interview)
    "scene" -> stringResource(R.string.extra_scene)
    "short" -> stringResource(R.string.extra_short)
    "clip" -> stringResource(R.string.extra_clip)
    else -> stringResource(R.string.extra_extra)
}

@kotlinx.serialization.Serializable
data class ItemIdsBody(@kotlinx.serialization.SerialName("item_ids") val itemIds: List<String>)

@kotlinx.serialization.Serializable
data class NewPlaylistBody(val name: String, @kotlinx.serialization.SerialName("item_ids") val itemIds: List<String>)

/** "Aggiungi a playlist": le proprie playlist, o una nuova. */
@Composable
fun AddToPlaylistDialog(itemId: String, onClose: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val lists = rememberLoad("playlists") { Api.get<PlaylistList>("/api/v1/playlists").items.filter { it.mine } }
    var name by remember { mutableStateOf("") }
    fun done(added: Int, title: String) {
        Toast.makeText(context, context.getString(R.string.playlist_added, title), Toast.LENGTH_SHORT).show()
        Session.bumpCatalog()
        onClose()
    }
    AlertDialog(
        onDismissRequest = onClose,
        containerColor = tokens.surface1,
        title = { Text(stringResource(R.string.playlist_add_to), color = tokens.ink) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                ResContent(lists) { ls ->
                    ls.forEach { p ->
                        SecondaryButton(p.name, modifier = Modifier.fillMaxWidth()) {
                            scope.launch {
                                runCatching { Api.send("/api/v1/playlists/${p.id}/items", "POST", ItemIdsBody(listOf(itemId))) }
                                    .onSuccess { done(1, p.name) }
                                    .onFailure { Toast.makeText(context, Api.message(it, ""), Toast.LENGTH_SHORT).show() }
                            }
                        }
                    }
                }
                OutlinedTextField(name, { name = it }, label = { Text(stringResource(R.string.playlist_new)) }, singleLine = true, colors = plateaFieldColors(), modifier = Modifier.fillMaxWidth())
            }
        },
        confirmButton = {
            TextButton(enabled = name.isNotBlank(), onClick = {
                scope.launch {
                    runCatching { Api.send("/api/v1/playlists", "POST", NewPlaylistBody(name.trim(), listOf(itemId))) }
                        .onSuccess { done(1, name.trim()) }
                        .onFailure { Toast.makeText(context, Api.message(it, ""), Toast.LENGTH_SHORT).show() }
                }
            }) { Text(stringResource(R.string.create), color = tokens.blueText) }
        },
        dismissButton = { TextButton(onClick = onClose) { Text(stringResource(R.string.cancel), color = tokens.inkMuted) } },
    )
}
