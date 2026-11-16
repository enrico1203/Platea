package app.platea.ui.components

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.layout
import androidx.compose.ui.graphics.drawscope.scale as drawScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import app.platea.R
import app.platea.data.Api
import app.platea.data.Item
import app.platea.data.Session
import app.platea.ui.theme.tokens
import app.platea.util.Fmt
import coil.compose.AsyncImage

/** Vero sui televisori (Android TV, Google TV, Fire TV): interfaccia da telecomando. */
val LocalIsTv = staticCompositionLocalOf { false }

/** Larghezza della finestra in dp: decide colonne e misure delle locandine. */
val LocalWidthDp = staticCompositionLocalOf { 400 }

/** Margine laterale delle pagine. */
@Composable
fun pagePadding(): Dp = when {
    LocalIsTv.current -> 48.dp
    LocalWidthDp.current >= 840 -> 32.dp
    LocalWidthDp.current >= 600 -> 24.dp
    else -> 16.dp
}

/** Misure delle locandine in base allo schermo. */
@Composable
fun posterWidth(wide: Boolean): Dp {
    val w = LocalWidthDp.current
    val tv = LocalIsTv.current
    return if (wide) {
        when { tv -> 260.dp; w >= 840 -> 280.dp; w >= 600 -> 240.dp; else -> 220.dp }
    } else {
        when { tv -> 150.dp; w >= 840 -> 160.dp; w >= 600 -> 140.dp; else -> 112.dp }
    }
}

/**
 * Stato visibile del focus per il telecomando: bordo blu e leggero ingrandimento. Su touch non cambia nulla.
 */
@Composable
fun Modifier.focusRing(interaction: MutableInteractionSource, shape: Shape = RoundedCornerShape(10.dp), grow: Boolean = true, glow: Boolean = false): Modifier {
    val focused by interaction.collectIsFocusedAsState()
    val s by animateFloatAsState(if (focused && grow) 1.05f else 1f, androidx.compose.animation.core.tween(160), label = "focus")
    val blue = tokens.blue
    return this
        .scale(s)
        // shadow-glow del marchio: la luce dello schermo, solo sulla locandina a fuoco e sul Play.
        .then(if (focused && glow) Modifier.shadow(24.dp, shape, ambientColor = blue, spotColor = blue) else Modifier)
        .then(if (focused) Modifier.border(BorderStroke(if (glow) 2.dp else 3.dp, if (glow) blue else tokens.focus), shape) else Modifier)
}

/** Risultato di un caricamento. */
sealed interface Res<out T> {
    data object Loading : Res<Nothing>
    data class Ok<T>(val value: T) : Res<T>
    data class Err(val message: String) : Res<Nothing>
}

class Loader<T>(val state: State<Res<T>>, val reload: () -> Unit) {
    val value: Res<T> get() = state.value
    val data: T? get() = (state.value as? Res.Ok<T>)?.value
}

/**
 * Carica dal server e ricarica quando cambiano le chiavi o quando il server segnala dati nuovi (WebSocket).
 * Durante una ricarica resta visibile il valore precedente.
 */
@Composable
fun <T> rememberLoad(vararg keys: Any?, load: suspend () -> T): Loader<T> {
    val tick by Session.catalogTick.collectAsState()
    var manual by remember { mutableIntStateOf(0) }
    val state = remember(*keys) { mutableStateOf<Res<T>>(Res.Loading) }
    val network = stringResource(R.string.error_network)
    LaunchedEffect(*keys, tick, manual) {
        try {
            state.value = Res.Ok(load())
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: Exception) {
            if (state.value !is Res.Ok) state.value = Res.Err(Api.message(e, network))
        }
    }
    return remember(state) { Loader(state) { manual++ } }
}

@Composable
fun LoadingBox(modifier: Modifier = Modifier) {
    Box(modifier.fillMaxWidth().padding(48.dp), contentAlignment = Alignment.Center) {
        CircularProgressIndicator(color = tokens.blueText, strokeWidth = 3.dp, modifier = Modifier.size(36.dp))
    }
}

@Composable
fun ErrorBox(message: String, onRetry: (() -> Unit)? = null, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxWidth().padding(32.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Text(message, color = tokens.inkMuted, style = MaterialTheme.typography.bodyLarge)
        if (onRetry != null) {
            Spacer(Modifier.height(16.dp))
            OutlinedButton(onClick = onRetry) { Text(stringResource(R.string.retry)) }
        }
    }
}

/** Stato vuoto: il marchio monocromatico e una frase che dice cosa fare. */
@Composable
fun EmptyBox(message: String, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 48.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Mark(48.dp, mono = true)
        Spacer(Modifier.height(16.dp))
        Text(
            message, color = tokens.inkMuted, style = MaterialTheme.typography.bodyLarge,
            textAlign = androidx.compose.ui.text.style.TextAlign.Center, modifier = Modifier.widthIn(max = 420.dp),
        )
    }
}

/** Contenuto standard di uno stato di caricamento. */
@Composable
fun <T> ResContent(loader: Loader<T>, content: @Composable (T) -> Unit) {
    when (val r = loader.value) {
        is Res.Loading -> LoadingBox()
        is Res.Err -> ErrorBox(r.message, loader.reload)
        is Res.Ok -> content(r.value)
    }
}

fun parseColor(hex: String?): Color? {
    if (hex.isNullOrBlank()) return null
    return runCatching { Color(android.graphics.Color.parseColor(hex)) }.getOrNull()
}

private const val MARK_P = "M14 10h28a10 10 0 0 1 10 10v5.5a10 10 0 0 1-10 10H22V52a2 2 0 0 1-2 2h-6a2 2 0 0 1-2-2V12a2 2 0 0 1 2-2zm8 6v13.5h20a4 4 0 0 0 4-4V20a4 4 0 0 0-4-4H22z"
private const val MARK_SCREEN = "M24,16h20a2,2 0 0 1 2,2v9.5a2,2 0 0 1 -2,2h-20a2,2 0 0 1 -2,-2v-9.5a2,2 0 0 1 2,-2z"

/**
 * Marchio: la P nel colore del testo del tema, l'occhiello è lo schermo blu. `mono` lascia l'occhiello vuoto
 * (stati vuoti, in `ink-subtle`, come vuole il marchio).
 */
@Composable
fun Mark(size: Dp = 32.dp, mono: Boolean = false, color: Color = if (mono) tokens.inkSubtle else tokens.ink) {
    val blue = tokens.blue
    val paths = remember {
        val p = androidx.compose.ui.graphics.vector.PathParser().parsePathString(MARK_P).toPath().apply { fillType = androidx.compose.ui.graphics.PathFillType.EvenOdd }
        val s = androidx.compose.ui.graphics.vector.PathParser().parsePathString(MARK_SCREEN).toPath()
        p to s
    }
    androidx.compose.foundation.Canvas(Modifier.size(size)) {
        val k = this.size.minDimension / 64f
        drawScale(k, k, pivot = androidx.compose.ui.geometry.Offset.Zero) {
            drawPath(paths.first, color)
            if (!mono) drawPath(paths.second, blue)
        }
    }
}

/**
 * Locandina 2:3 (o miniatura 16:9 per episodi e "continua a guardare"): angoli `radius-sm`, filo interno, il fondo è
 * il colore segnaposto finché l'immagine non arriva. A fuoco col telecomando: filo blu e la luce dello schermo.
 */
@Composable
fun PosterCard(
    item: Item,
    wide: Boolean = false,
    width: Dp? = null,
    meta: String? = null,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    val w = width ?: posterWidth(wide)
    val interaction = remember { MutableInteractionSource() }
    val src = if (wide) item.images.still ?: item.images.backdrop ?: item.images.poster else item.images.poster
    val shape = RoundedCornerShape(6.dp)
    val title = if (item.kind == "episode" && wide && item.seriesTitle != null) item.seriesTitle else item.title
    val line = meta ?: posterMeta(item, wide)
    Column(
        modifier
            .width(w)
            .clickable(interactionSource = interaction, indication = null, onClick = onClick),
    ) {
        Box(
            Modifier
                .fillMaxWidth()
                .aspectRatio(if (wide) 16f / 9f else 2f / 3f)
                .focusRing(interaction, shape, glow = true)
                .clip(shape)
                .background(parseColor(item.images.posterColor) ?: tokens.placeholder),
        ) {
            if (src != null) {
                AsyncImage(
                    model = Api.image(src, if (wide) 480 else 320),
                    contentDescription = null,
                    contentScale = ContentScale.Crop,
                    modifier = Modifier.fillMaxSize(),
                )
            } else {
                Column(Modifier.align(Alignment.Center).padding(10.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    Mark(28.dp, mono = true)
                    Spacer(Modifier.height(8.dp))
                    Text(title, color = tokens.inkMuted, style = MaterialTheme.typography.labelMedium, maxLines = 3, overflow = TextOverflow.Ellipsis, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
                }
            }
            if (wide && item.kind == "episode") {
                // Sulla miniatura dell'episodio il codice resta leggibile anche senza leggere la riga sotto.
                Text(
                    "S${item.parentIndexNumber ?: 0} E${item.indexNumber ?: 0}", style = app.platea.ui.theme.Mono.small, color = Color.White,
                    modifier = Modifier.align(Alignment.BottomStart).padding(8.dp).background(Color(0xB3000000), RoundedCornerShape(4.dp)).padding(horizontal = 6.dp, vertical = 2.dp),
                )
            }
            if (item.progress > 0f) ProgressLine(item.progress, Modifier.align(Alignment.BottomStart))
            if (item.userData.played) WatchedBadge(Modifier.align(Alignment.TopEnd).padding(6.dp))
            if (!item.available) {
                Box(Modifier.fillMaxSize().background(Color(0x99000000)), contentAlignment = Alignment.Center) {
                    Text(stringResource(R.string.unavailable), color = Color.White, style = MaterialTheme.typography.labelMedium)
                }
            }
            // Filo interno: separa la locandina dal fondo anche quando l'immagine è scura.
            Box(Modifier.matchParentSize().border(1.dp, Color.White.copy(alpha = 0.07f), shape))
        }
        Spacer(Modifier.height(8.dp))
        Text(title, style = MaterialTheme.typography.labelLarge, color = tokens.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (line.isNotEmpty()) {
            Text(line, style = MaterialTheme.typography.bodySmall, color = tokens.inkSubtle, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable
fun posterMeta(item: Item, wide: Boolean): String = when (item.kind) {
    "episode" -> if (wide) item.title else item.episodeCode
    "series" -> listOfNotNull(stringResource(R.string.kind_series), item.childCount?.takeIf { it > 0 }?.let { pluralRes(R.plurals.seasons, it) }).joinToString(" · ")
    "collection" -> listOfNotNull(stringResource(R.string.kind_collection), item.childCount?.takeIf { it > 0 }?.let { pluralRes(R.plurals.titles, it) }).joinToString(" · ")
    "season" -> item.childCount?.let { pluralRes(R.plurals.episodes, it) } ?: ""
    else -> listOfNotNull(item.year?.toString(), item.runtimeMs?.let { Fmt.duration(it) }).joinToString(" · ")
}

@Composable
fun pluralRes(id: Int, count: Int): String = androidx.compose.ui.res.pluralStringResource(id, count, count)

@Composable
fun ProgressLine(progress: Float, modifier: Modifier = Modifier) {
    Box(modifier.fillMaxWidth().height(4.dp).background(Color(0x99000000))) {
        Box(Modifier.fillMaxWidth(progress.coerceIn(0f, 1f)).height(4.dp).background(tokens.blue))
    }
}

@Composable
fun WatchedBadge(modifier: Modifier = Modifier) {
    Box(modifier.size(22.dp).clip(CircleShape).background(tokens.blue), contentAlignment = Alignment.Center) {
        Icon(Icons.Filled.Check, contentDescription = stringResource(R.string.watched), tint = Color.White, modifier = Modifier.size(15.dp))
    }
}

/** Titolo di sezione (`h2`) con azione facoltativa ("Tutti ›"). */
@Composable
fun SectionHeader(title: String, action: String? = null, onAction: (() -> Unit)? = null) {
    val big = LocalIsTv.current || LocalWidthDp.current >= 600
    Row(
        Modifier.fillMaxWidth().padding(horizontal = pagePadding()).padding(top = if (big) 32.dp else 24.dp, bottom = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            title, style = if (big) MaterialTheme.typography.headlineSmall else MaterialTheme.typography.titleLarge,
            color = tokens.ink, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis,
        )
        if (action != null && onAction != null) {
            val interaction = remember { MutableInteractionSource() }
            Row(
                Modifier.focusRing(interaction, RoundedCornerShape(8.dp), grow = false).clip(RoundedCornerShape(8.dp))
                    .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple(), onClick = onAction)
                    .padding(start = 8.dp, top = 4.dp, bottom = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(action, style = MaterialTheme.typography.labelLarge, color = tokens.blueText)
                Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, tint = tokens.blueText, modifier = Modifier.size(20.dp))
            }
        }
    }
}

/** Etichetta `eyebrow` sopra un gruppo: maiuscolo, `ink-subtle`. */
@Composable
fun Eyebrow(text: String, modifier: Modifier = Modifier, color: Color = tokens.inkSubtle) {
    Text(text.uppercase(), style = MaterialTheme.typography.labelSmall, color = color, modifier = modifier)
}

/** Riga orizzontale di locandine. */
@Composable
fun PosterRow(items: List<Item>, wide: Boolean = false, onClick: (Item) -> Unit) {
    LazyRow(
        contentPadding = PaddingValues(horizontal = pagePadding(), vertical = if (LocalIsTv.current) 10.dp else 0.dp),
        horizontalArrangement = Arrangement.spacedBy(if (LocalIsTv.current) 24.dp else 12.dp),
    ) {
        items(items, key = { it.id + (it.entryId ?: "") }) { PosterCard(it, wide = wide) { onClick(it) } }
    }
}

/** Etichetta piccola (genere, "Mancante"…). */
@Composable
fun Badge(text: String, color: Color = tokens.inkMuted, background: Color = tokens.surface2) {
    Surface(shape = RoundedCornerShape(4.dp), color = background) {
        Text(text, color = color, style = MaterialTheme.typography.labelMedium, modifier = Modifier.padding(horizontal = 8.dp, vertical = 3.dp))
    }
}

/** Tag tecnico (`pl-badge--codec`): risoluzione, HDR, codec, canali. Mono, filo `line-strong`. */
@Composable
fun CodecBadge(text: String, color: Color = tokens.inkMuted) {
    Text(
        text, style = app.platea.ui.theme.Mono.small, color = color,
        modifier = Modifier.border(1.dp, tokens.lineStrong, RoundedCornerShape(4.dp)).padding(horizontal = 6.dp, vertical = 2.dp),
    )
}

/** Metodo di riproduzione: verde direct play, blu remux, arancio transcodifica (come nella web UI). */
@Composable
fun MethodBadge(method: String) {
    val (label, color) = when (method) {
        "direct_play" -> stringResource(R.string.method_direct) to tokens.success
        "remux" -> stringResource(R.string.method_remux) to tokens.blueText
        else -> stringResource(R.string.method_transcode) to tokens.warning
    }
    Surface(shape = RoundedCornerShape(4.dp), color = color.copy(alpha = 0.12f), border = BorderStroke(1.dp, color.copy(alpha = 0.45f))) {
        Text(label, color = color, style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(horizontal = 8.dp, vertical = 3.dp))
    }
}

/** Pulsante principale (`pl-btn--primary`): uno solo per vista. `large` = 48 dp (dettaglio, telefono). */
@Composable
fun PrimaryButton(text: String, icon: androidx.compose.ui.graphics.vector.ImageVector? = null, enabled: Boolean = true, modifier: Modifier = Modifier, large: Boolean = false, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(10.dp)
    Button(
        onClick = onClick,
        enabled = enabled,
        interactionSource = interaction,
        shape = shape,
        colors = ButtonDefaults.buttonColors(
            containerColor = tokens.blue, contentColor = Color.White,
            disabledContainerColor = tokens.blue.copy(alpha = 0.35f), disabledContentColor = Color.White.copy(alpha = 0.7f),
        ),
        modifier = modifier.heightIn(min = if (large) 48.dp else 44.dp).focusRing(interaction, shape, grow = false),
        contentPadding = PaddingValues(horizontal = if (large) 24.dp else 18.dp, vertical = 0.dp),
    ) {
        if (icon != null) { Icon(icon, null, Modifier.size(20.dp)); Spacer(Modifier.width(8.dp)) }
        Text(text, style = MaterialTheme.typography.labelLarge, fontWeight = FontWeight.SemiBold, fontSize = if (large) 15.sp else 14.sp)
    }
}

/** Pulsante secondario (`pl-btn--secondary`): `surface-2` con filo `line-strong`. */
@Composable
fun SecondaryButton(text: String, icon: androidx.compose.ui.graphics.vector.ImageVector? = null, enabled: Boolean = true, modifier: Modifier = Modifier, large: Boolean = false, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(10.dp)
    Button(
        onClick = onClick,
        enabled = enabled,
        interactionSource = interaction,
        shape = shape,
        border = BorderStroke(1.dp, tokens.lineStrong),
        colors = ButtonDefaults.buttonColors(
            containerColor = tokens.surface2, contentColor = tokens.ink,
            disabledContainerColor = tokens.surface2, disabledContentColor = tokens.inkSubtle,
        ),
        modifier = modifier.heightIn(min = if (large) 48.dp else 44.dp).focusRing(interaction, shape, grow = false),
        contentPadding = PaddingValues(horizontal = if (large) 22.dp else 16.dp, vertical = 0.dp),
    ) {
        if (icon != null) { Icon(icon, null, Modifier.size(20.dp)); Spacer(Modifier.width(8.dp)) }
        Text(text, style = MaterialTheme.typography.labelLarge, fontSize = if (large) 15.sp else 14.sp)
    }
}

/** Pulsante senza fondo (`pl-btn--ghost`): Annulla, azioni secondarie nelle barre. */
@Composable
fun GhostButton(text: String, icon: androidx.compose.ui.graphics.vector.ImageVector? = null, modifier: Modifier = Modifier, color: Color = tokens.inkMuted, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(10.dp)
    Row(
        modifier.heightIn(min = 44.dp).focusRing(interaction, shape, grow = false).clip(shape)
            .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple(), onClick = onClick)
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        if (icon != null) { Icon(icon, null, Modifier.size(20.dp), tint = color); Spacer(Modifier.width(8.dp)) }
        Text(text, style = MaterialTheme.typography.labelLarge, color = color)
    }
}

/**
 * Azione con icona ed etichetta sotto (visto, preferito, playlist, scarica): si capisce cosa fa senza tenere premuto,
 * e sul telecomando il focus è un riquadro grande.
 */
@Composable
fun ActionButton(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    label: String,
    active: Boolean = false,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    val interaction = remember { MutableInteractionSource() }
    val shape = RoundedCornerShape(10.dp)
    Column(
        modifier.widthIn(min = 72.dp).focusRing(interaction, shape, grow = false).clip(shape)
            .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple(), onClick = onClick)
            .padding(horizontal = 6.dp, vertical = 8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Icon(icon, contentDescription = null, tint = if (active) tokens.blueText else tokens.ink, modifier = Modifier.size(24.dp))
        Spacer(Modifier.height(6.dp))
        Text(label, style = MaterialTheme.typography.labelMedium, color = if (active) tokens.blueText else tokens.inkMuted, maxLines = 1)
    }
}

/** Pulsante tondo con icona (visto, preferito…). */
@Composable
fun RoundIconButton(
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    label: String,
    active: Boolean = false,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    val interaction = remember { MutableInteractionSource() }
    Box(
        modifier
            .size(46.dp)
            .focusRing(interaction, CircleShape)
            .clip(CircleShape)
            .background(if (active) tokens.blueSoft else tokens.surface2)
            .border(1.dp, if (active) tokens.blue else tokens.line, CircleShape)
            .clickable(interactionSource = interaction, indication = androidx.compose.material3.ripple(), onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = label, tint = if (active) tokens.blueText else tokens.ink, modifier = Modifier.size(22.dp))
    }
}

/**
 * Allarga il contenuto oltre il margine del contenitore (le righe che scorrono di lato dentro una griglia con
 * margini): il contenuto va da bordo a bordo e rimette il margine con il suo `contentPadding`.
 */
fun Modifier.bleed(horizontal: Dp): Modifier = this.then(
    Modifier.layout { measurable, constraints ->
        val extra = horizontal.roundToPx() * 2
        val placeable = measurable.measure(
            constraints.copy(
                minWidth = (constraints.minWidth + extra).coerceAtLeast(0),
                maxWidth = if (constraints.hasBoundedWidth) constraints.maxWidth + extra else constraints.maxWidth,
            ),
        )
        layout(placeable.width - extra, placeable.height) { placeable.place(-extra / 2, 0) }
    },
)
