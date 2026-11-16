package app.platea.ui.screens

import android.app.DownloadManager
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Environment
import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.border
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
import androidx.compose.foundation.border
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Replay
import androidx.compose.material.icons.outlined.CheckCircle
import androidx.compose.material.icons.outlined.FileDownload
import androidx.compose.material.icons.outlined.Movie
import androidx.compose.ui.platform.LocalConfiguration
import app.platea.ui.components.ActionButton
import app.platea.ui.components.CodecBadge
import app.platea.ui.components.Eyebrow
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
            val backdrop = item.images.backdrop ?: item.images.still
            val screenH = LocalConfiguration.current.screenHeightDp
            val bg = tokens.bg
            val big = tv || wide
            val eyebrow = when (item.kind) {
                "movie" -> stringResource(R.string.kind_movie_label)
                "series" -> stringResource(R.string.kind_series)
                "season" -> item.seriesTitle ?: stringResource(R.string.kind_season_label)
                "episode" -> listOfNotNull(item.seriesTitle, "S${item.parentIndexNumber} · E${item.indexNumber}").joinToString("  ·  ")
                "collection" -> stringResource(R.string.kind_collection)
                else -> ""
            }
            Box(Modifier.fillMaxWidth()) {
                // Sfondo a tutta larghezza che sfuma nel fondo: su telefono il titolo ci sta sopra, in basso.
                Box(Modifier.fillMaxWidth().height(if (tv) 300.dp else if (wide) 420.dp else (screenH * 0.5f).dp.coerceIn(300.dp, 460.dp))) {
                    Box(Modifier.fillMaxSize().background(parseColor(item.images.posterColor)?.copy(alpha = 0.3f) ?: tokens.placeholder))
                    if (backdrop != null) {
                        AsyncImage(Api.image(backdrop, 1600), null, contentScale = ContentScale.Crop, alignment = Alignment.TopCenter, modifier = Modifier.fillMaxSize())
                    }
                    Box(Modifier.fillMaxSize().background(Brush.verticalGradient(0f to bg.copy(alpha = 0.45f), 0.2f to Color.Transparent, 0.55f to bg.copy(alpha = 0.3f), 1f to bg)))
                    if (big) Box(Modifier.fillMaxSize().background(Brush.horizontalGradient(0f to bg.copy(alpha = 0.7f), 0.6f to Color.Transparent)))
                }
                IconButton(
                    onClick = { nav.back() },
                    modifier = Modifier.windowInsetsPadding(WindowInsets.statusBars).padding(start = pagePadding() - 8.dp, top = 4.dp).background(tokens.scrim, CircleShape),
                ) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back), tint = Color.White)
                }
                Row(
                    Modifier.padding(horizontal = pagePadding()).padding(top = if (tv) 90.dp else if (wide) 230.dp else ((screenH * 0.5f).coerceIn(300f, 460f) - 150).dp),
                    verticalAlignment = Alignment.Bottom,
                ) {
                    if (big && item.kind != "episode") {
                        Box(
                            Modifier.width(if (tv) 150.dp else 190.dp).aspectRatio(2f / 3f).clip(RoundedCornerShape(6.dp))
                                .background(parseColor(item.images.posterColor) ?: tokens.placeholder)
                                .border(1.dp, Color.White.copy(alpha = 0.08f), RoundedCornerShape(6.dp)),
                        ) {
                            item.images.poster?.let { AsyncImage(Api.image(it, 400), null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize()) }
                        }
                        Spacer(Modifier.width(28.dp))
                    }
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (eyebrow.isNotEmpty()) {
                            if (item.seriesId != null && item.kind != "series") {
                                Text(
                                    eyebrow.uppercase(), style = MaterialTheme.typography.labelSmall, color = tokens.blueText,
                                    modifier = Modifier.clickable { nav.item(item.seriesId) },
                                )
                            } else Eyebrow(eyebrow, color = tokens.inkMuted)
                        }
                        val logo = item.images.logo
                        if (logo != null && item.kind != "episode") {
                            AsyncImage(Api.image(logo, 600), item.title, contentScale = ContentScale.Fit, alignment = Alignment.CenterStart, modifier = Modifier.heightIn(max = if (big) 110.dp else 88.dp).widthIn(max = 360.dp))
                        } else {
                            Text(
                                item.title, style = if (big) MaterialTheme.typography.displayMedium else MaterialTheme.typography.displaySmall,
                                color = tokens.ink, maxLines = 3, overflow = TextOverflow.Ellipsis,
                            )
                        }
                        if (meta.isNotEmpty()) Text(meta.joinToString("  ·  "), style = MaterialTheme.typography.bodyMedium, color = tokens.inkMuted)
                        file?.let { f ->
                            val tags = techTags(f)
                            if (tags.isNotEmpty()) {
                                FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) { tags.forEach { CodecBadge(it) } }
                            }
                        }
                    }
                }
            }
        }

        item("info") {
            // Righe di testo lunghe si leggono male sui tablet e sulle TV: la colonna delle informazioni ha un limite.
            Column(Modifier.padding(horizontal = pagePadding()).padding(top = 20.dp).widthIn(max = 880.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                if (expired) {
                    Surface(color = tokens.warning.copy(alpha = 0.12f), shape = RoundedCornerShape(10.dp)) {
                        Text(stringResource(R.string.item_expired), color = tokens.warning, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.padding(12.dp))
                    }
                }
                val canPlay = !expired && (item.isPlayable || nextEpisode != null)
                val playLabel = when {
                    resume -> stringResource(R.string.item_resume_from, Fmt.duration(item.userData.positionMs))
                    item.isPlayable -> stringResource(R.string.play)
                    nextEpisode != null -> stringResource(R.string.play) + " " + nextEpisode.episodeCode
                    else -> ""
                }
                val play = { if (item.isPlayable) nav.play(item.id, file?.id) else nextEpisode?.let { nav.play(it.id) } }
                // Su telefono il Play occupa tutta la riga (pollice); su tablet e TV sta accanto alle azioni.
                if (!wide && !tv && canPlay) {
                    PrimaryButton(playLabel, Icons.Filled.PlayArrow, modifier = Modifier.fillMaxWidth().focusRequester(playFocus), large = true) { play() }
                    if (resume) {
                        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            if (item.runtimeMs != null) Box(Modifier.fillMaxWidth().clip(RoundedCornerShape(2.dp))) { ProgressLine(item.userData.positionMs.toFloat() / item.runtimeMs) }
                            SecondaryButton(stringResource(R.string.item_from_start), Icons.Filled.Replay, modifier = Modifier.fillMaxWidth()) { nav.play(item.id, file?.id, startOver = true) }
                        }
                    }
                }
                FlowRow(horizontalArrangement = Arrangement.spacedBy(if (wide || tv) 10.dp else 4.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    if ((wide || tv) && canPlay) {
                        PrimaryButton(playLabel, Icons.Filled.PlayArrow, modifier = Modifier.focusRequester(playFocus), large = true) { play() }
                        if (resume) SecondaryButton(stringResource(R.string.item_from_start), Icons.Filled.Replay, large = true) { nav.play(item.id, file?.id, startOver = true) }
                        Spacer(Modifier.width(6.dp))
                    }
                    if (item.trailers.isNotEmpty()) {
                        ActionButton(Icons.Outlined.Movie, stringResource(R.string.item_trailer)) { openTrailer(context, item.trailers.first().key) }
                    }
                    if (item.kind != "collection") {
                        ActionButton(if (item.userData.played) Icons.Filled.CheckCircle else Icons.Outlined.CheckCircle, stringResource(R.string.watched), item.userData.played) {
                            toggle("played", !item.userData.played)
                        }
                        ActionButton(if (item.userData.favorite) Icons.Filled.Favorite else Icons.Filled.FavoriteBorder, stringResource(R.string.favorite), item.userData.favorite) {
                            toggle("favorite", !item.userData.favorite)
                        }
                    }
                    ActionButton(Icons.AutoMirrored.Filled.PlaylistAdd, stringResource(R.string.action_playlist)) { dialog = "playlist" }
                    if (item.isPlayable && me?.permissions?.allowDownload == true && !expired && !tv) {
                        ActionButton(Icons.Outlined.FileDownload, stringResource(R.string.action_download)) {
                            scope.launch { download(context, item.id, file?.id) }
                        }
                    }
                }
                if ((wide || tv) && resume && item.runtimeMs != null) Box(Modifier.widthIn(max = 360.dp).clip(RoundedCornerShape(2.dp))) { ProgressLine(item.userData.positionMs.toFloat() / item.runtimeMs) }
                item.tagline?.takeIf { it.isNotBlank() }?.let { Text(it, style = MaterialTheme.typography.titleMedium, color = tokens.ink) }
                item.overview?.takeIf { it.isNotBlank() }?.let { Text(it, style = MaterialTheme.typography.bodyLarge, color = tokens.inkMuted) }
                if (item.genres.isNotEmpty()) {
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        item.genres.forEach { g -> Chip(g) { nav.genre(g) } }
                    }
                }
                if (item.studios.isNotEmpty() && item.kind != "episode") {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Eyebrow(stringResource(R.string.studios))
                        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            item.studios.take(4).forEach { s -> Chip(s) { nav.studio(s) } }
                        }
                    }
                }
                if (item.collections.isNotEmpty()) {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Eyebrow(stringResource(R.string.part_of))
                        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            item.collections.forEach { c -> Chip(c.title) { nav.item(c.id) } }
                        }
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
        color = if (active) tokens.blueSoft else tokens.surface2, shape = shape,
        border = if (active) androidx.compose.foundation.BorderStroke(1.dp, tokens.blue) else null,
        modifier = Modifier.focusRing(interaction, shape, grow = false).clip(shape)
            .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple(), onClick = onClick),
    ) {
        Text(title, style = MaterialTheme.typography.labelLarge, color = if (active) tokens.blueText else tokens.ink, modifier = Modifier.padding(horizontal = 16.dp, vertical = 9.dp))
    }
}

@Composable
private fun EpisodeRow(e: Item, onOpen: () -> Unit, onPlay: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(12.dp)
    val wide = LocalWidthDp.current >= 600
    Row(
        Modifier.fillMaxWidth().padding(horizontal = pagePadding() - 6.dp, vertical = 4.dp)
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
    Row(Modifier.fillMaxWidth().padding(horizontal = pagePadding() - 6.dp, vertical = 4.dp).padding(6.dp), verticalAlignment = Alignment.CenterVertically) {
        // Episodio che manca dalla libreria: solo il contorno del riquadro, così non pesa quanto quelli presenti.
        Box(
            Modifier.width(if (LocalWidthDp.current >= 600) 200.dp else 140.dp).aspectRatio(16f / 9f).clip(RoundedCornerShape(8.dp))
                .border(1.dp, tokens.line, RoundedCornerShape(8.dp)),
            contentAlignment = Alignment.Center,
        ) {
            Text("${m.episode}", style = MaterialTheme.typography.titleLarge, color = tokens.line)
        }
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
        Modifier.width(if (LocalIsTv.current) 116.dp else 84.dp).clickable(interactionSource = interaction, indication = null, onClick = onClick),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(Modifier.fillMaxWidth().aspectRatio(1f).focusRing(interaction, CircleShape).clip(CircleShape).background(tokens.placeholder), contentAlignment = Alignment.Center) {
            if (image != null) AsyncImage(Api.image(image, 200), null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
            else Text(name.split(' ').mapNotNull { it.firstOrNull()?.uppercase() }.take(2).joinToString(""), color = tokens.inkMuted, style = MaterialTheme.typography.titleMedium)
        }
        Spacer(Modifier.height(6.dp))
        Text(name, style = MaterialTheme.typography.labelMedium, color = tokens.ink, maxLines = 2, overflow = TextOverflow.Ellipsis, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
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
    // Pannello (`surface-1`, filo `line-strong`): i dati tecnici in mono, come nella dashboard.
    Surface(color = tokens.surface1, shape = RoundedCornerShape(14.dp), border = androidx.compose.foundation.BorderStroke(1.dp, tokens.line), modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(f.name, style = app.platea.ui.theme.Mono.regular, color = tokens.inkSubtle)
            Box(Modifier.fillMaxWidth().height(1.dp).background(tokens.line))
            rows.forEach { (k, v) ->
                Row {
                    Text(k.uppercase(), style = MaterialTheme.typography.labelSmall, color = tokens.inkSubtle, modifier = Modifier.width(96.dp).padding(top = 2.dp))
                    Text(v, style = app.platea.ui.theme.Mono.regular, color = tokens.ink)
                }
            }
        }
    }
}

/** Tag tecnici della versione: risoluzione, HDR, codec video, audio. */
fun techTags(f: MediaFile): List<String> {
    val v = f.video.firstOrNull()
    val a = f.audio.firstOrNull { it.default } ?: f.audio.firstOrNull()
    val res = v?.let { s ->
        val w = s.width ?: 0; val h = s.height ?: 0
        when { w >= 3200 || h >= 1800 -> "4K"; w >= 1800 || h >= 1000 -> "1080p"; w >= 1200 || h >= 700 -> "720p"; w > 0 -> "SD"; else -> null }
    }
    val hdr = v?.hdr?.takeIf { it.isNotBlank() }?.let { h ->
        when (h.lowercase()) { "dovi", "dolby_vision", "dv" -> "Dolby Vision"; "hdr10plus", "hdr10+" -> "HDR10+"; "hlg" -> "HLG"; else -> h.uppercase() }
    }
    val vc = v?.codec?.let { c -> when (c.lowercase()) { "h264", "avc" -> "H.264"; "hevc", "h265" -> "HEVC"; "av1" -> "AV1"; "vp9" -> "VP9"; else -> c.uppercase() } }
    val ac = a?.let { s ->
        val name = when (s.codec.lowercase()) { "eac3" -> "DD+"; "ac3" -> "DD"; "truehd" -> "TrueHD"; "dts" -> "DTS"; "aac" -> "AAC"; "flac" -> "FLAC"; "opus" -> "Opus"; "mp3" -> "MP3"; else -> s.codec.uppercase() }
        listOfNotNull(name, s.channels?.let { Fmt.channels(it) }).joinToString(" ")
    }
    return listOfNotNull(res, hdr, vc, ac)
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
                OutlinedTextField(name, { name = it }, label = { Text(stringResource(R.string.playlist_new)) }, singleLine = true, colors = plateaFieldColors(), shape = app.platea.ui.theme.Radius.md, modifier = Modifier.fillMaxWidth())
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
