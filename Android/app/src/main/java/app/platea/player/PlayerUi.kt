package app.platea.player

import android.graphics.Bitmap
import android.view.ViewGroup
import android.widget.FrameLayout
import androidx.annotation.OptIn
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.FormatListBulleted
import androidx.compose.material.icons.filled.ClosedCaption
import androidx.compose.material.icons.filled.FastForward
import androidx.compose.material.icons.filled.Forward10
import androidx.compose.material.icons.filled.Forward30
import androidx.compose.material.icons.filled.Forward5
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.FastRewind
import androidx.compose.material.icons.filled.Replay10
import androidx.compose.material.icons.filled.Replay30
import androidx.compose.material.icons.filled.Replay5
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.SkipNext
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.media3.common.util.UnstableApi
import androidx.media3.ui.AspectRatioFrameLayout
import androidx.media3.ui.CaptionStyleCompat
import androidx.media3.ui.PlayerView
import app.platea.R
import app.platea.data.Api
import app.platea.data.Trickplay
import app.platea.ui.components.LocalIsTv
import app.platea.ui.components.MethodBadge
import app.platea.ui.theme.tokens
import app.platea.util.Fmt
import coil.imageLoader
import coil.request.ImageRequest
import coil.request.SuccessResult
import kotlinx.coroutines.delay

private val QUALITIES = listOf(0L, 40_000_000L, 20_000_000L, 8_000_000L, 4_000_000L, 2_000_000L, 1_000_000L)
private val Muted = Color(0xFFA2A9B4)
private val Subtle = Color(0xFF808893)

@OptIn(UnstableApi::class)
@Composable
fun PlayerScreen(c: PlayerController, controls: ControlsState, onBack: () -> Unit) {
    val tv = LocalIsTv.current
    val prefs = c.prefs

    // I controlli spariscono dopo qualche secondo di riproduzione senza tocchi.
    LaunchedEffect(controls.visible, controls.lastPoke, c.playing, controls.panel) {
        if (!controls.visible || !c.playing || controls.panel != Panel.None) return@LaunchedEffect
        delay(if (tv) 5000 else 3500)
        if (c.playing && controls.panel == Panel.None) controls.hide()
    }
    LaunchedEffect(controls.seekHint) {
        if (controls.seekHint != null) { delay(700); controls.seekHint = null }
    }

    Box(Modifier.fillMaxSize().background(Color.Black)) {
        AndroidView(
            factory = { ctx ->
                PlayerView(ctx).apply {
                    useController = false
                    layoutParams = FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
                    resizeMode = AspectRatioFrameLayout.RESIZE_MODE_FIT
                    setShutterBackgroundColor(android.graphics.Color.BLACK)
                    setKeepContentOnPlayerReset(true)
                    isFocusable = false
                    subtitleView?.apply {
                        val bg = if (prefs.subtitleBackground) 0xB3000000.toInt() else android.graphics.Color.TRANSPARENT
                        setStyle(CaptionStyleCompat(android.graphics.Color.WHITE, bg, android.graphics.Color.TRANSPARENT,
                            if (prefs.subtitleBackground) CaptionStyleCompat.EDGE_TYPE_NONE else CaptionStyleCompat.EDGE_TYPE_OUTLINE,
                            android.graphics.Color.BLACK, null))
                        val size = when (prefs.subtitleSize) { "s" -> 0.045f; "l" -> 0.068f; "xl" -> 0.082f; else -> 0.056f }
                        setFractionalTextSize(size)
                        setApplyEmbeddedStyles(false)
                    }
                    player = c.player
                }
            },
            modifier = Modifier.fillMaxSize(),
        )

        // Superficie dei tocchi: un tocco mostra o nasconde, doppio tocco ai lati salta.
        if (!tv) {
            BoxWithConstraints(Modifier.fillMaxSize()) {
                val w = constraints.maxWidth
                Box(
                    Modifier.fillMaxSize().pointerInput(w) {
                        detectTapGestures(
                            onTap = { if (controls.visible) controls.hide() else controls.poke() },
                            onDoubleTap = { o ->
                                when {
                                    o.x < w / 3f -> { c.skip(-prefs.skipBackSeconds); controls.seekHint = "−${prefs.skipBackSeconds} s"; controls.seekHintLeft = true }
                                    o.x > 2 * w / 3f -> { c.skip(prefs.skipForwardSeconds); controls.seekHint = "+${prefs.skipForwardSeconds} s"; controls.seekHintLeft = false }
                                    else -> c.togglePlay()
                                }
                            },
                        )
                    },
                )
            }
        }

        if (c.buffering && c.error == null && !controls.pip) {
            CircularProgressIndicator(color = Color.White, strokeWidth = 3.dp, modifier = Modifier.align(Alignment.Center).size(52.dp))
        }

        controls.seekHint?.let { hint ->
            Box(Modifier.fillMaxSize(), contentAlignment = if (controls.seekHintLeft) Alignment.CenterStart else Alignment.CenterEnd) {
                Text(
                    hint, color = Color.White, style = MaterialTheme.typography.titleLarge,
                    modifier = Modifier.padding(horizontal = 64.dp).background(Color(0x99000000), RoundedCornerShape(999.dp)).padding(horizontal = 18.dp, vertical = 10.dp),
                )
            }
        }

        AnimatedVisibility(controls.visible && !controls.pip && c.error == null, enter = fadeIn(), exit = fadeOut()) {
            Controls(c, controls, onBack)
        }

        if (controls.panel != Panel.None && !controls.pip) SidePanel(c, controls)

        c.notice?.let { n ->
            Text(
                n, color = Color.White, style = MaterialTheme.typography.bodyLarge,
                modifier = Modifier.align(Alignment.TopCenter).windowInsetsPadding(WindowInsets.safeDrawing).padding(top = 72.dp)
                    .background(Color(0xCC14171B), RoundedCornerShape(12.dp)).padding(horizontal = 18.dp, vertical = 12.dp),
            )
        }

        c.upNext?.let { UpNextCard(c, it, Modifier.align(Alignment.BottomEnd)) }
        c.error?.let { ErrorOverlay(c, it, onBack) }
    }
}

@Composable
private fun Controls(c: PlayerController, controls: ControlsState, onBack: () -> Unit) {
    val tv = LocalIsTv.current
    val item = c.item
    val title = if (item?.kind == "episode") item.seriesTitle ?: item.title else item?.title ?: ""
    val sub = when {
        item == null -> ""
        item.kind == "episode" -> "S${item.parentIndexNumber} · E${item.indexNumber} · ${item.title}"
        else -> item.year?.toString() ?: ""
    }
    val playFocus = remember { FocusRequester() }
    LaunchedEffect(Unit) { if (tv) runCatching { playFocus.requestFocus() } }

    Box(Modifier.fillMaxSize()) {
        Box(Modifier.fillMaxSize().background(Brush.verticalGradient(0f to Color(0xB3000000), 0.25f to Color.Transparent, 0.65f to Color.Transparent, 1f to Color(0xD9000000))))
        Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing).padding(horizontal = if (tv) 48.dp else 12.dp, vertical = if (tv) 28.dp else 8.dp)) {
            // In alto: indietro, titolo, metodo.
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (!tv) CtrlButton(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back), onClick = onBack)
                Column(Modifier.weight(1f).padding(horizontal = 8.dp)) {
                    Text(title, color = Color.White, style = MaterialTheme.typography.titleLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    if (sub.isNotEmpty()) Text(sub, color = Muted, style = MaterialTheme.typography.bodySmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                c.info?.let { MethodBadge(it.method) }
            }
            Spacer(Modifier.weight(1f))
            // Al centro: salti e play.
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
                CtrlButton(backIcon(c.prefs.skipBackSeconds), stringResource(R.string.player_back_n, c.prefs.skipBackSeconds), size = 56) { c.skip(-c.prefs.skipBackSeconds); controls.poke() }
                Spacer(Modifier.width(28.dp))
                PlayButton(c.playing, Modifier.focusRequester(playFocus)) { c.togglePlay(); controls.poke() }
                Spacer(Modifier.width(28.dp))
                CtrlButton(forwardIcon(c.prefs.skipForwardSeconds), stringResource(R.string.player_forward_n, c.prefs.skipForwardSeconds), size = 56) { c.skip(c.prefs.skipForwardSeconds); controls.poke() }
            }
            Spacer(Modifier.weight(1f))
            SeekBar(c, controls)
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("${Fmt.clock(c.positionMs)} / ${Fmt.clock(c.durationMs)}", color = Color.White, style = MaterialTheme.typography.labelLarge, fontFamily = app.platea.ui.theme.GeistMono)
                Spacer(Modifier.weight(1f))
                if (c.hasNext) CtrlButton(Icons.Filled.SkipNext, stringResource(R.string.player_next)) { c.next() }
                if (c.chapters.size > 1) CtrlButton(Icons.AutoMirrored.Filled.FormatListBulleted, stringResource(R.string.player_chapters)) { controls.panel = Panel.Chapters }
                CtrlButton(Icons.Filled.ClosedCaption, stringResource(R.string.player_subtitles_audio)) { controls.panel = Panel.Subtitles }
                CtrlButton(Icons.Filled.Settings, stringResource(R.string.player_quality)) { controls.panel = Panel.Quality }
                CtrlButton(Icons.Filled.Info, stringResource(R.string.player_info)) { controls.panel = Panel.Info }
            }
        }
    }
}

@Composable
private fun CtrlButton(icon: ImageVector, label: String, size: Int = 44, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    Box(
        Modifier.size(size.dp).clip(CircleShape)
            .background(if (focused) Color.White else Color.Transparent)
            .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple(color = Color.White), onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, label, tint = if (focused) Color.Black else Color.White, modifier = Modifier.size((size * 0.58f).dp))
    }
}

@Composable
private fun PlayButton(playing: Boolean, modifier: Modifier, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    Box(
        modifier.size(76.dp)
            .shadow(28.dp, CircleShape, ambientColor = tokens.blue, spotColor = tokens.blue)
            .clip(CircleShape).background(tokens.blue)
            .then(if (focused) Modifier.border(3.dp, Color.White, CircleShape) else Modifier)
            .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple(color = Color.White), onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            if (playing) Icons.Filled.Pause else Icons.Filled.PlayArrow,
            stringResource(if (playing) R.string.player_pause else R.string.play), tint = Color.White, modifier = Modifier.size(42.dp),
        )
    }
}

/**
 * Barra di avanzamento: trascinamento al tocco; col telecomando sinistra/destra spostano un punto d'arrivo che si
 * applica dopo una breve pausa (o con OK). Anteprime dalle miniature se il server le ha.
 */
@Composable
private fun SeekBar(c: PlayerController, controls: ControlsState) {
    val duration = c.durationMs
    var scrub by remember { mutableStateOf<Long?>(null) }
    var focused by remember { mutableStateOf(false) }
    val shown = scrub ?: c.positionMs
    val step = if (duration > 3_600_000) 15_000L else 10_000L
    val interaction = remember { MutableInteractionSource() }

    LaunchedEffect(scrub, focused) {
        val s = scrub ?: return@LaunchedEffect
        if (!focused) return@LaunchedEffect
        delay(900)
        c.seekTo(s)
        scrub = null
    }

    BoxWithConstraints(Modifier.fillMaxWidth().height(if (focused || scrub != null) 120.dp else 40.dp)) {
        val widthPx = constraints.maxWidth.toFloat()
        val density = LocalDensity.current
        fun posFor(x: Float) = if (duration > 0) ((x / widthPx).coerceIn(0f, 1f) * duration).toLong() else 0L
        // Anteprima sopra la barra.
        val tp = c.info?.trickplay
        if (tp != null && scrub != null && duration > 0) {
            val previewW = with(density) { (if (LocalIsTv.current) 260.dp else 180.dp).toPx() }
            val x = (shown.toFloat() / duration * widthPx - previewW / 2).coerceIn(0f, (widthPx - previewW).coerceAtLeast(0f))
            Box(Modifier.offset { IntOffset(x.toInt(), 0) }) { TrickplayThumb(tp, shown, with(density) { previewW.toDp() }) }
        }
        Box(
            Modifier.align(Alignment.BottomStart).fillMaxWidth().height(40.dp)
                .onFocusChanged { focused = it.isFocused; if (!it.isFocused && scrub != null) { c.seekTo(scrub!!); scrub = null } }
                .onPreviewKeyEvent { e ->
                    if (e.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                    when (e.key) {
                        Key.DirectionLeft -> { scrub = ((scrub ?: c.positionMs) - step).coerceAtLeast(0); controls.poke(); true }
                        Key.DirectionRight -> { scrub = ((scrub ?: c.positionMs) + step).coerceAtMost(duration); controls.poke(); true }
                        Key.DirectionCenter, Key.Enter -> { scrub?.let { c.seekTo(it); scrub = null } ?: c.togglePlay(); controls.poke(); true }
                        else -> false
                    }
                }
                .focusable(interactionSource = interaction)
                .pointerInput(duration) {
                    detectTapGestures { o -> c.seekTo(posFor(o.x)); controls.poke() }
                }
                .pointerInput(duration) {
                    detectHorizontalDragGestures(
                        onDragStart = { o -> scrub = posFor(o.x); controls.poke() },
                        onDragEnd = { scrub?.let { c.seekTo(it) }; scrub = null },
                        onDragCancel = { scrub = null },
                        onHorizontalDrag = { change, _ -> scrub = posFor(change.position.x); controls.poke() },
                    )
                },
            contentAlignment = Alignment.CenterStart,
        ) {
            val blue = tokens.blue
            val chapters = c.chapters
            val buffered = c.bufferedMs
            Canvas(Modifier.fillMaxWidth().height(24.dp)) {
                val h = if (focused || scrub != null) 8f * density.density / 1.5f else 4f * density.density / 1.5f
                val y = size.height / 2
                val frac = if (duration > 0) (shown.toFloat() / duration).coerceIn(0f, 1f) else 0f
                val buf = if (duration > 0) (buffered.toFloat() / duration).coerceIn(0f, 1f) else 0f
                drawRect(Color(0x4DFFFFFF), Offset(0f, y - h / 2), Size(size.width, h))
                drawRect(Color(0x66FFFFFF), Offset(0f, y - h / 2), Size(size.width * buf, h))
                drawRect(blue, Offset(0f, y - h / 2), Size(size.width * frac, h))
                if (duration > 0 && chapters.size > 1) {
                    chapters.drop(1).forEach { ch ->
                        val cx = ch.startMs.toFloat() / duration * size.width
                        drawRect(Color.Black, Offset(cx - 1.5f, y - h / 2), Size(3f, h))
                    }
                }
                val r = if (focused || scrub != null) 9f * density.density else 6f * density.density
                drawCircle(if (focused) Color.White else blue, r, Offset(size.width * frac, y))
            }
        }
        if (scrub != null && tp == null) {
            Text(
                Fmt.clock(shown), color = Color.White, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold,
                modifier = Modifier.align(Alignment.TopCenter).background(Color(0xAA000000), RoundedCornerShape(8.dp)).padding(horizontal = 12.dp, vertical = 6.dp),
            )
        }
    }
}

/** Miniatura del punto indicato, ritagliata dalla sua griglia, con l'orario sotto. */
@Composable
private fun TrickplayThumb(tp: Trickplay, posMs: Long, width: androidx.compose.ui.unit.Dp) {
    val context = LocalContext.current
    val idx = (posMs / tp.intervalMs).toInt().coerceIn(0, (tp.count - 1).coerceAtLeast(0))
    val perSheet = tp.cols * tp.rows
    val sheet = idx / perSheet
    val cell = idx % perSheet
    val sheets = remember(tp.url) { mutableStateMapOf<Int, ImageBitmap>() }
    LaunchedEffect(tp.url, sheet) {
        if (sheets.containsKey(sheet)) return@LaunchedEffect
        val url = Api.url(tp.url.replace("{n}", sheet.toString())) ?: return@LaunchedEffect
        val r = context.imageLoader.execute(ImageRequest.Builder(context).data(url).allowHardware(false).build())
        val bmp = ((r as? SuccessResult)?.drawable as? android.graphics.drawable.BitmapDrawable)?.bitmap
        if (bmp != null) sheets[sheet] = bmp.copy(Bitmap.Config.ARGB_8888, false).asImageBitmap()
    }
    val height = width * (tp.height.toFloat() / tp.width)
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Box(Modifier.width(width).height(height).clip(RoundedCornerShape(8.dp)).background(Color(0xFF14171B)).border(2.dp, Color.White, RoundedCornerShape(8.dp))) {
            sheets[sheet]?.let { img ->
                Canvas(Modifier.fillMaxSize()) {
                    val sx = (cell % tp.cols) * tp.width
                    val sy = (cell / tp.cols) * tp.height
                    drawImage(img, srcOffset = IntOffset(sx, sy), srcSize = IntSize(tp.width.coerceAtMost(img.width - sx), tp.height.coerceAtMost(img.height - sy)), dstSize = IntSize(size.width.toInt(), size.height.toInt()))
                }
            }
        }
        Text(Fmt.clock(posMs), color = Color.White, style = MaterialTheme.typography.labelLarge, modifier = Modifier.padding(top = 2.dp).background(Color(0xAA000000), RoundedCornerShape(6.dp)).padding(horizontal = 8.dp, vertical = 2.dp))
    }
}

@Composable
private fun SidePanel(c: PlayerController, controls: ControlsState) {
    val tv = LocalIsTv.current
    val first = remember { FocusRequester() }
    LaunchedEffect(controls.panel) { delay(50); runCatching { first.requestFocus() } }
    Box(Modifier.fillMaxSize().clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { controls.panel = Panel.None }) {
        Surface(
            color = Color(0xFA0C0E11),
            border = androidx.compose.foundation.BorderStroke(1.dp, Color(0xFF22262D)),
            shape = RoundedCornerShape(topStart = 16.dp, bottomStart = 16.dp),
            modifier = Modifier.align(Alignment.CenterEnd).fillMaxHeight().widthIn(max = if (tv) 420.dp else 340.dp).fillMaxWidth(0.85f)
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {},
        ) {
            val info = c.info
            LazyColumn(Modifier.windowInsetsPadding(WindowInsets.safeDrawing).padding(vertical = 16.dp)) {
                when (controls.panel) {
                    Panel.Chapters -> {
                        item { PanelTitle(stringResource(R.string.player_chapters)) }
                        val list = c.chapters
                        items(list.size) { i ->
                            val ch = list[i]
                            val next = list.getOrNull(i + 1)?.startMs ?: c.durationMs
                            val current = c.positionMs in ch.startMs until next
                            PanelRow(ch.title ?: stringResource(R.string.player_chapter_n, i + 1), Fmt.clock(ch.startMs), current, if (i == 0) Modifier.focusRequester(first) else Modifier) {
                                c.seekTo(ch.startMs); controls.panel = Panel.None
                            }
                        }
                    }
                    Panel.Subtitles -> if (info != null) {
                        item { PanelTitle(stringResource(R.string.subtitles)) }
                        item { PanelRow(stringResource(R.string.off), null, c.subIndex == null, Modifier.focusRequester(first)) { c.selectSubtitle(null); controls.panel = Panel.None } }
                        items(info.subtitles, key = { "s${it.index}" }) { s ->
                            PanelRow(
                                listOfNotNull(Fmt.language(s.language), s.title).joinToString(" · "),
                                listOfNotNull(s.codec.replace("hdmv_pgs_subtitle", "pgs").uppercase(), if (s.forced) stringResource(R.string.forced) else null).joinToString(" · "),
                                c.subIndex == s.index,
                            ) { c.selectSubtitle(s.index); controls.panel = Panel.None }
                        }
                        if (info.audioTracks.size > 1) {
                            item { PanelTitle(stringResource(R.string.audio)) }
                            items(info.audioTracks, key = { "a${it.index}" }) { a ->
                                PanelRow(Fmt.language(a.language), "${a.codec.uppercase()} ${Fmt.channels(a.channels)}", info.audioIndex == a.index) {
                                    c.selectAudio(a.index); controls.panel = Panel.None
                                }
                            }
                        }
                    }
                    Panel.Quality -> {
                        item { PanelTitle(stringResource(R.string.player_quality)) }
                        item {
                            val now = info?.let { if (it.maxBitrate != null) Fmt.bitrate(it.maxBitrate) else stringResource(R.string.quality_original) }
                            PanelRow(stringResource(R.string.quality_auto), if (c.quality == null) now else null, c.quality == null, Modifier.focusRequester(first)) {
                                c.selectQuality(null); controls.panel = Panel.None
                            }
                        }
                        items(QUALITIES) { q ->
                            PanelRow(if (q == 0L) stringResource(R.string.quality_original) else Fmt.bitrate(q), null, c.quality == q) {
                                c.selectQuality(q); controls.panel = Panel.None
                            }
                        }
                    }
                    Panel.Info -> if (info != null) {
                        item { PanelTitle(stringResource(R.string.player_info)) }
                        item {
                            Column(Modifier.padding(horizontal = 20.dp).focusRequester(first).focusable(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                                MethodBadge(info.method)
                                Text(info.chain, color = Muted, fontFamily = app.platea.ui.theme.GeistMono, style = MaterialTheme.typography.bodySmall)
                                if (info.reasons.isNotEmpty()) Text(info.reasons.joinToString(", "), color = Subtle, fontFamily = app.platea.ui.theme.GeistMono, style = MaterialTheme.typography.bodySmall)
                                Text(
                                    stringResource(when (info.method) { "direct_play" -> R.string.why_direct; "remux" -> R.string.why_remux; else -> R.string.why_transcode }),
                                    color = Muted, style = MaterialTheme.typography.bodySmall,
                                )
                                val vs = c.player.videoSize
                                if (vs.width > 0) Text("${vs.width}×${vs.height} · ${c.player.videoFormat?.sampleMimeType ?: ""} · ${c.player.audioFormat?.sampleMimeType ?: ""}", color = Subtle, style = MaterialTheme.typography.bodySmall)
                            }
                        }
                    }
                    Panel.None -> {}
                }
            }
        }
    }
}

@Composable
private fun PanelTitle(t: String) {
    Text(t.uppercase(), color = Subtle, style = MaterialTheme.typography.labelSmall, modifier = Modifier.padding(start = 20.dp, end = 20.dp, top = 16.dp, bottom = 8.dp))
}

@Composable
private fun PanelRow(label: String, detail: String?, active: Boolean, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    Row(
        modifier.fillMaxWidth().padding(horizontal = 8.dp).clip(RoundedCornerShape(10.dp))
            .background(if (focused) Color(0xFF2F343C) else if (active) Color(0xFF0F1D3D) else Color.Transparent)
            .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple(), onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, color = if (active) Color(0xFF6E9BFF) else Color.White, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (detail != null) Text(detail, color = Subtle, style = MaterialTheme.typography.bodySmall, fontFamily = app.platea.ui.theme.GeistMono)
    }
}

@Composable
private fun UpNextCard(c: PlayerController, n: NextUp, modifier: Modifier) {
    val focus = remember { FocusRequester() }
    LaunchedEffect(n) { runCatching { focus.requestFocus() } }
    Surface(color = Color(0xF214171B), shape = RoundedCornerShape(14.dp), modifier = modifier.windowInsetsPadding(WindowInsets.safeDrawing).padding(24.dp).widthIn(max = 380.dp)) {
        Column(Modifier.padding(18.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.player_up_next_in, c.countdown), color = Muted, style = MaterialTheme.typography.bodySmall)
            Text(n.title, color = Color.White, style = MaterialTheme.typography.titleMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                app.platea.ui.components.PrimaryButton(stringResource(R.string.player_play_now), Icons.Filled.PlayArrow, modifier = Modifier.focusRequester(focus)) { c.playNext(n) }
                app.platea.ui.components.SecondaryButton(stringResource(R.string.cancel)) { c.cancelUpNext() }
            }
        }
    }
}

@Composable
private fun ErrorOverlay(c: PlayerController, message: String, onBack: () -> Unit) {
    val focus = remember { FocusRequester() }
    LaunchedEffect(message) { runCatching { focus.requestFocus() } }
    Box(Modifier.fillMaxSize().background(Color(0xCC000000)), contentAlignment = Alignment.Center) {
        Column(Modifier.padding(24.dp).widthIn(max = 520.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(stringResource(R.string.player_cannot_play), color = Color.White, style = MaterialTheme.typography.titleLarge)
            Text(message, color = Muted, style = MaterialTheme.typography.bodyMedium)
            c.errorDetail?.let { Text(it, color = Subtle, fontFamily = app.platea.ui.theme.GeistMono, style = MaterialTheme.typography.bodySmall) }
            Spacer(Modifier.height(8.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                app.platea.ui.components.SecondaryButton(stringResource(R.string.back)) { onBack() }
                app.platea.ui.components.PrimaryButton(stringResource(R.string.retry), modifier = Modifier.focusRequester(focus)) { c.retry() }
            }
        }
    }
}

/** Icona di salto con il numero giusto, se Material ce l'ha. */
private fun backIcon(s: Int) = when (s) { 5 -> Icons.Filled.Replay5; 10 -> Icons.Filled.Replay10; 30 -> Icons.Filled.Replay30; else -> Icons.Filled.FastRewind }
private fun forwardIcon(s: Int) = when (s) { 5 -> Icons.Filled.Forward5; 10 -> Icons.Filled.Forward10; 30 -> Icons.Filled.Forward30; else -> Icons.Filled.FastForward }
