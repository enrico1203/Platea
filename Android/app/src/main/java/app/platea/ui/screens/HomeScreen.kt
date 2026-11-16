package app.platea.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.platea.R
import app.platea.data.Api
import app.platea.data.Home
import app.platea.data.Item
import app.platea.data.ItemDetail
import app.platea.data.Session
import app.platea.ui.LocalNav
import app.platea.ui.components.EmptyBox
import app.platea.ui.components.LocalIsTv
import app.platea.ui.components.LocalWidthDp
import app.platea.ui.components.PosterRow
import app.platea.ui.components.PrimaryButton
import app.platea.ui.components.ProgressLine
import app.platea.ui.components.ResContent
import app.platea.ui.components.SecondaryButton
import app.platea.ui.components.SectionHeader
import app.platea.ui.components.pagePadding
import app.platea.ui.components.rememberLoad
import app.platea.ui.theme.tokens
import app.platea.util.Fmt
import coil.compose.AsyncImage

@Composable
fun HomeScreen() {
    val home = rememberLoad("home") { Api.get<Home>("/api/v1/home") }
    val nav = LocalNav.current
    val me = Session.me
    ResContent(home) { data ->
        val nothing = data.resume.isEmpty() && data.nextUp.isEmpty() && data.recent.all { it.items.isEmpty() }
        if (nothing) {
            EmptyBox(stringResource(if (me?.isAdmin == true) R.string.home_empty_admin else R.string.home_empty), Modifier.windowInsetsPadding(WindowInsets.statusBars))
            return@ResContent
        }
        val order = me?.preferences?.homeSections ?: listOf("resume", "next_up", "recent")
        val featured = pickFeatured(if ("resume" in order) data.resume else emptyList(), data.recent.flatMap { it.items })
        val resumeRow = data.resume.filter { !(featured?.second == true && it.id == featured.first.id) }
        LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = 32.dp)) {
            if (featured != null) {
                item("hero") { Hero(featured.first, featured.second) }
            } else {
                item("top") { Spacer(Modifier.windowInsetsPadding(WindowInsets.statusBars).height(8.dp)) }
            }
            for (section in order) {
                when (section) {
                    "resume" -> if (resumeRow.isNotEmpty()) {
                        item("resume") {
                            SectionHeader(stringResource(R.string.home_resume))
                            PosterRow(resumeRow, wide = true) { nav.play(it.id) }
                        }
                    }
                    "next_up" -> if (data.nextUp.isNotEmpty()) {
                        item("next_up") {
                            SectionHeader(stringResource(R.string.home_next_up))
                            PosterRow(data.nextUp, wide = true) { nav.item(it.id) }
                        }
                    }
                    "recent" -> data.recent.filter { it.items.isNotEmpty() }.forEach { r ->
                        item("recent-${r.libraryId}") {
                            SectionHeader(stringResource(R.string.home_recent_in, r.libraryName), stringResource(R.string.all)) { nav.library(r.libraryId) }
                            PosterRow(r.items, wide = r.kind == "shows") { nav.item(it.id) }
                        }
                    }
                }
            }
        }
    }
}

/**
 * "In scena": ciò che si stava guardando, altrimenti un titolo appena arrivato con uno sfondo, stabile per tutta la
 * giornata. Il secondo valore dice se è una ripresa.
 */
fun pickFeatured(resume: List<Item>, recent: List<Item>): Pair<Item, Boolean>? {
    resume.firstOrNull { it.images.backdrop != null }?.let { return it to true }
    val pool = recent.filter { it.images.backdrop != null && (it.kind == "movie" || it.kind == "series") }
    if (pool.isEmpty()) return null
    val day = (System.currentTimeMillis() / 86_400_000L).toInt()
    return pool[day % pool.size] to false
}

@Composable
private fun Hero(item: Item, resume: Boolean) {
    val nav = LocalNav.current
    val tv = LocalIsTv.current
    val wide = LocalWidthDp.current >= 600
    val detailId = if (item.kind == "episode" && item.seriesId != null) item.seriesId else item.id
    val detail = rememberLoad("hero", detailId) { Api.get<ItemDetail>("/api/v1/items/$detailId") }.data
    val title = if (item.kind == "episode") item.seriesTitle ?: item.title else item.title
    val left = item.runtimeMs?.let { (it - item.userData.positionMs).coerceAtLeast(0) } ?: 0
    val eyebrow = if (resume) {
        listOfNotNull(stringResource(R.string.hero_resume), if (item.kind == "episode") item.episodeCode else null, if (left > 0) stringResource(R.string.hero_left, Fmt.duration(left)) else null)
    } else listOf(stringResource(R.string.hero_added, Fmt.date(item.addedAt)))
    val meta = listOfNotNull(
        (detail?.year ?: item.year)?.toString(),
        if (item.kind == "movie") item.runtimeMs?.let { Fmt.duration(it) } else null,
        detail?.officialRating,
        detail?.communityRating?.let { Fmt.rating(it) },
    ) + (detail?.genres ?: emptyList()).take(3)
    val overview = if (item.kind == "episode") item.overview else detail?.overview ?: item.overview
    val playFocus = remember { FocusRequester() }
    LaunchedEffect(Unit) { if (tv) runCatching { playFocus.requestFocus() } }

    Box(Modifier.fillMaxWidth().heightIn(min = if (tv) 420.dp else if (wide) 440.dp else 520.dp)) {
        AsyncImage(
            model = Api.image(item.images.backdrop, 1600),
            contentDescription = null,
            contentScale = ContentScale.Crop,
            modifier = Modifier.matchParentSize(),
        )
        // Lo sfondo sfuma nel fondo della pagina, e a sinistra dietro il testo.
        Box(Modifier.matchParentSize().background(Brush.verticalGradient(0f to Color.Transparent, 0.45f to tokens.bg.copy(alpha = 0.35f), 1f to tokens.bg)))
        Box(Modifier.matchParentSize().background(Brush.horizontalGradient(0f to tokens.bg.copy(alpha = 0.75f), 0.6f to Color.Transparent)))
        Column(
            Modifier.align(Alignment.BottomStart).padding(horizontal = pagePadding()).padding(bottom = 8.dp).widthIn(max = 640.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text(eyebrow.joinToString(" · "), style = MaterialTheme.typography.labelLarge, color = tokens.inkMuted)
            val logo = detail?.images?.logo
            if (logo != null) {
                AsyncImage(model = Api.image(logo, 600), contentDescription = title, modifier = Modifier.heightIn(max = 96.dp).widthIn(max = 360.dp), contentScale = ContentScale.Fit, alignment = Alignment.CenterStart)
            } else {
                Text(title, style = MaterialTheme.typography.displaySmall, color = tokens.ink, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
            if (meta.isNotEmpty()) Text(meta.joinToString("  ·  "), style = MaterialTheme.typography.bodySmall, color = tokens.inkMuted)
            if (!overview.isNullOrBlank()) {
                Text(overview, style = MaterialTheme.typography.bodyMedium, color = tokens.inkMuted, maxLines = 3, overflow = TextOverflow.Ellipsis)
            }
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                PrimaryButton(stringResource(if (resume) R.string.hero_resume_action else R.string.play), Icons.Filled.PlayArrow, modifier = Modifier.focusRequester(playFocus)) {
                    nav.play(item.id)
                }
                SecondaryButton(stringResource(R.string.hero_details), Icons.Outlined.Info) { nav.item(detailId) }
            }
            if (resume && item.runtimeMs != null && item.runtimeMs > 0) {
                Box(Modifier.widthIn(max = 320.dp)) { ProgressLine(item.progress) }
            }
        }
    }
}
