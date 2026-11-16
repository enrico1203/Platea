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
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.interaction.collectIsDraggedAsState
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.getValue
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.platform.LocalConfiguration
import app.platea.ui.components.Eyebrow
import app.platea.ui.components.parseColor
import kotlinx.coroutines.delay
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
    val tv = LocalIsTv.current
    ResContent(home) { data ->
        val nothing = data.resume.isEmpty() && data.nextUp.isEmpty() && data.recent.all { it.items.isEmpty() }
        if (nothing) {
            EmptyBox(stringResource(if (me?.isAdmin == true) R.string.home_empty_admin else R.string.home_empty), Modifier.windowInsetsPadding(WindowInsets.statusBars).padding(top = 64.dp))
            return@ResContent
        }
        val order = me?.preferences?.homeSections ?: listOf("resume", "next_up", "recent")
        val featured = pickFeatured(if ("resume" in order) data.resume else emptyList(), data.recent.flatMap { it.items }, if (tv) 1 else 5)
        val resumeRow = data.resume.filter { r -> featured.none { it.second && it.first.id == r.id } }
        LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = 40.dp)) {
            if (featured.isNotEmpty()) {
                item("hero") { HeroCarousel(featured) }
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
 * "In scena": prima ciò che si stava guardando, poi titoli appena arrivati con uno sfondo. L'ordine dei nuovi cambia
 * ogni giorno ma resta stabile durante la giornata. Il secondo valore dice se è una ripresa.
 */
fun pickFeatured(resume: List<Item>, recent: List<Item>, max: Int = 5): List<Pair<Item, Boolean>> {
    val out = mutableListOf<Pair<Item, Boolean>>()
    resume.firstOrNull { it.images.backdrop != null }?.let { out += it to true }
    val pool = recent.filter { it.images.backdrop != null && (it.kind == "movie" || it.kind == "series") }.distinctBy { it.id }
    if (pool.isNotEmpty()) {
        val day = (System.currentTimeMillis() / 86_400_000L).toInt()
        val start = day % pool.size
        for (i in pool.indices) {
            if (out.size >= max) break
            val it = pool[(start + i) % pool.size]
            if (out.none { o -> o.first.id == it.id }) out += it to false
        }
    }
    return out.take(max)
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun HeroCarousel(items: List<Pair<Item, Boolean>>) {
    val tv = LocalIsTv.current
    val wide = LocalWidthDp.current >= 600
    val screenH = LocalConfiguration.current.screenHeightDp
    val height = when {
        tv -> 440.dp
        wide -> (screenH * 0.62f).dp.coerceIn(420.dp, 620.dp)
        else -> (screenH * 0.68f).dp.coerceIn(460.dp, 640.dp)
    }
    if (items.size == 1) {
        Hero(items[0].first, items[0].second, Modifier.fillMaxWidth().height(height))
        return
    }
    val pager = rememberPagerState { items.size }
    // Avanza da solo, ma mai mentre la si sta scorrendo a mano.
    val dragged by pager.interactionSource.collectIsDraggedAsState()
    LaunchedEffect(pager, dragged) {
        if (dragged) return@LaunchedEffect
        while (true) {
            delay(8000)
            pager.animateScrollToPage((pager.currentPage + 1) % items.size, animationSpec = tween(600))
        }
    }
    Box(Modifier.fillMaxWidth().height(height)) {
        HorizontalPager(pager, Modifier.fillMaxSize(), key = { items[it].first.id }) { page ->
            Hero(items[page].first, items[page].second, Modifier.fillMaxSize())
        }
        Row(
            Modifier.align(Alignment.BottomEnd).padding(end = pagePadding(), bottom = 22.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            repeat(items.size) { i ->
                val active = pager.currentPage == i
                val w by animateDpAsState(if (active) 22.dp else 8.dp, tween(200), label = "dot")
                Box(Modifier.width(w).height(4.dp).clip(RoundedCornerShape(2.dp)).background(if (active) tokens.ink else tokens.ink.copy(alpha = 0.3f)))
            }
        }
    }
}

@Composable
private fun Hero(item: Item, resume: Boolean, modifier: Modifier) {
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
    ) + (detail?.genres ?: emptyList()).take(if (wide) 3 else 2)
    val overview = if (item.kind == "episode") item.overview else detail?.overview ?: item.overview
    val playFocus = remember { FocusRequester() }
    LaunchedEffect(Unit) { if (tv) runCatching { playFocus.requestFocus() } }
    val bg = tokens.bg

    Box(modifier.clipToBounds()) {
        Box(Modifier.matchParentSize().background(parseColor(item.images.posterColor)?.copy(alpha = 0.35f) ?: tokens.placeholder))
        AsyncImage(
            model = Api.image(item.images.backdrop, if (wide || tv) 1920 else 1280),
            contentDescription = null,
            contentScale = ContentScale.Crop,
            alignment = if (wide || tv) Alignment.TopEnd else Alignment.TopCenter,
            modifier = Modifier.matchParentSize(),
        )
        // Lo sfondo scende verso `bg` (velo del marchio): forte in basso, dietro il testo; a sinistra su schermi larghi.
        Box(Modifier.matchParentSize().background(Brush.verticalGradient(0f to bg.copy(alpha = 0.35f), 0.22f to Color.Transparent, 0.5f to bg.copy(alpha = 0.25f), 0.82f to bg.copy(alpha = 0.92f), 1f to bg)))
        if (wide || tv) Box(Modifier.matchParentSize().background(Brush.horizontalGradient(0f to bg.copy(alpha = 0.85f), 0.55f to Color.Transparent)))
        Column(
            Modifier.align(Alignment.BottomStart).padding(horizontal = pagePadding()).padding(bottom = if (tv) 24.dp else 44.dp).widthIn(max = 620.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Eyebrow(eyebrow.joinToString("  ·  "), color = tokens.inkMuted)
            val logo = detail?.images?.logo
            if (logo != null) {
                AsyncImage(model = Api.image(logo, 600), contentDescription = title, modifier = Modifier.heightIn(max = if (wide || tv) 110.dp else 84.dp).widthIn(max = 380.dp), contentScale = ContentScale.Fit, alignment = Alignment.CenterStart)
            } else {
                Text(
                    title, style = if (wide || tv) MaterialTheme.typography.displayMedium else MaterialTheme.typography.displaySmall,
                    color = tokens.ink, maxLines = 2, overflow = TextOverflow.Ellipsis,
                )
            }
            if (meta.isNotEmpty()) Text(meta.joinToString("  ·  "), style = MaterialTheme.typography.bodySmall, color = tokens.inkMuted)
            if (!overview.isNullOrBlank()) {
                Text(overview, style = MaterialTheme.typography.bodyMedium, color = tokens.inkMuted, maxLines = if (wide || tv) 3 else 2, overflow = TextOverflow.Ellipsis)
            }
            if (resume && item.runtimeMs != null && item.runtimeMs > 0) {
                Box(Modifier.widthIn(max = 320.dp).clip(RoundedCornerShape(2.dp))) { ProgressLine(item.progress) }
            }
            Spacer(Modifier.height(4.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                PrimaryButton(stringResource(if (resume) R.string.hero_resume_action else R.string.play), Icons.Filled.PlayArrow, modifier = Modifier.focusRequester(playFocus), large = true) {
                    nav.play(item.id)
                }
                SecondaryButton(stringResource(R.string.hero_details), Icons.Outlined.Info, large = true) { nav.item(detailId) }
            }
        }
    }
}
