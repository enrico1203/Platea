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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
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
fun Modifier.focusRing(interaction: MutableInteractionSource, shape: Shape = RoundedCornerShape(10.dp), grow: Boolean = true): Modifier {
    val focused by interaction.collectIsFocusedAsState()
    val s by animateFloatAsState(if (focused && grow) 1.06f else 1f, label = "focus")
    return this
        .scale(s)
        .then(if (focused) Modifier.border(BorderStroke(3.dp, tokens.focus), shape) else Modifier)
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

@Composable
fun EmptyBox(message: String, modifier: Modifier = Modifier) {
    Box(modifier.fillMaxWidth().padding(48.dp), contentAlignment = Alignment.Center) {
        Text(message, color = tokens.inkMuted, style = MaterialTheme.typography.bodyLarge)
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

/** Marchio vettoriale. */
@Composable
fun Mark(size: Dp = 32.dp) {
    Image(painterResource(R.drawable.ic_mark), contentDescription = null, modifier = Modifier.size(size))
}

/**
 * Locandina 2:3 (o miniatura 16:9 per episodi e "continua a guardare"). Il fondo è il colore segnaposto finché
 * l'immagine non arriva.
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
    val shape = RoundedCornerShape(10.dp)
    val title = if (item.kind == "episode" && wide && item.seriesTitle != null) item.seriesTitle else item.title
    val line = meta ?: posterMeta(item, wide)
    Column(
        modifier
            .then(if (width != null || !wide) Modifier.width(w) else Modifier.width(w))
            .clickable(interactionSource = interaction, indication = null, onClick = onClick),
    ) {
        Box(
            Modifier
                .fillMaxWidth()
                .aspectRatio(if (wide) 16f / 9f else 2f / 3f)
                .focusRing(interaction, shape)
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
                Text(
                    title, color = tokens.inkMuted, style = MaterialTheme.typography.labelLarge,
                    modifier = Modifier.align(Alignment.Center).padding(8.dp), maxLines = 3, overflow = TextOverflow.Ellipsis,
                )
            }
            if (item.progress > 0f) ProgressLine(item.progress, Modifier.align(Alignment.BottomStart))
            if (item.userData.played) WatchedBadge(Modifier.align(Alignment.TopEnd).padding(6.dp))
            if (!item.available) {
                Box(Modifier.fillMaxSize().background(Color(0x99000000)), contentAlignment = Alignment.Center) {
                    Text(stringResource(R.string.unavailable), color = Color.White, style = MaterialTheme.typography.labelMedium)
                }
            }
        }
        Spacer(Modifier.height(6.dp))
        Text(title, style = MaterialTheme.typography.labelLarge, color = tokens.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (line.isNotEmpty()) {
            Text(line, style = MaterialTheme.typography.bodySmall, color = tokens.inkSubtle, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable
fun posterMeta(item: Item, wide: Boolean): String = when (item.kind) {
    "episode" -> item.episodeCode + if (wide) " · ${item.title}" else ""
    "series" -> listOfNotNull(stringResource(R.string.kind_series), item.childCount?.takeIf { it > 0 }?.let { pluralRes(R.plurals.seasons, it) }).joinToString(" · ")
    "collection" -> listOfNotNull(stringResource(R.string.kind_collection), item.childCount?.takeIf { it > 0 }?.let { pluralRes(R.plurals.titles, it) }).joinToString(" · ")
    "season" -> item.childCount?.let { pluralRes(R.plurals.episodes, it) } ?: ""
    else -> listOfNotNull(item.year?.toString(), item.runtimeMs?.let { Fmt.duration(it) }).joinToString(" · ")
}

@Composable
fun pluralRes(id: Int, count: Int): String = androidx.compose.ui.res.pluralStringResource(id, count, count)

@Composable
fun ProgressLine(progress: Float, modifier: Modifier = Modifier) {
    Box(modifier.fillMaxWidth().height(4.dp).background(Color(0x66000000))) {
        Box(Modifier.fillMaxWidth(progress).height(4.dp).background(tokens.blue))
    }
}

@Composable
fun WatchedBadge(modifier: Modifier = Modifier) {
    Box(modifier.size(22.dp).clip(CircleShape).background(tokens.blue), contentAlignment = Alignment.Center) {
        Icon(Icons.Filled.Check, contentDescription = stringResource(R.string.watched), tint = Color.White, modifier = Modifier.size(15.dp))
    }
}

/** Titolo di sezione con azione facoltativa ("Tutti"). */
@Composable
fun SectionHeader(title: String, action: String? = null, onAction: (() -> Unit)? = null) {
    Row(
        Modifier.fillMaxWidth().padding(horizontal = pagePadding()).padding(top = 20.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(title, style = MaterialTheme.typography.titleLarge, color = tokens.ink, modifier = Modifier.weight(1f))
        if (action != null && onAction != null) {
            TextButton(onClick = onAction) { Text(action, color = tokens.blueText) }
        }
    }
}

/** Riga orizzontale di locandine. */
@Composable
fun PosterRow(items: List<Item>, wide: Boolean = false, onClick: (Item) -> Unit) {
    LazyRow(
        contentPadding = PaddingValues(horizontal = pagePadding(), vertical = if (LocalIsTv.current) 10.dp else 0.dp),
        horizontalArrangement = Arrangement.spacedBy(if (LocalIsTv.current) 20.dp else 12.dp),
    ) {
        items(items, key = { it.id + (it.entryId ?: "") }) { PosterCard(it, wide = wide) { onClick(it) } }
    }
}

/** Etichetta piccola (genere, "Mancante"…). */
@Composable
fun Badge(text: String, color: Color = tokens.inkMuted, background: Color = tokens.surface2) {
    Surface(shape = RoundedCornerShape(6.dp), color = background) {
        Text(text, color = color, style = MaterialTheme.typography.labelMedium, modifier = Modifier.padding(horizontal = 8.dp, vertical = 3.dp))
    }
}

/** Metodo di riproduzione: verde direct play, blu remux, arancio transcodifica (come nella web UI). */
@Composable
fun MethodBadge(method: String) {
    val (label, color) = when (method) {
        "direct_play" -> stringResource(R.string.method_direct) to tokens.success
        "remux" -> stringResource(R.string.method_remux) to tokens.blueText
        else -> stringResource(R.string.method_transcode) to tokens.warning
    }
    Surface(shape = RoundedCornerShape(6.dp), color = color.copy(alpha = 0.16f), border = BorderStroke(1.dp, color.copy(alpha = 0.5f))) {
        Text(label, color = color, style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(horizontal = 8.dp, vertical = 3.dp))
    }
}

/** Pulsante principale blu con focus visibile. */
@Composable
fun PrimaryButton(text: String, icon: androidx.compose.ui.graphics.vector.ImageVector? = null, enabled: Boolean = true, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    Button(
        onClick = onClick,
        enabled = enabled,
        interactionSource = interaction,
        shape = RoundedCornerShape(10.dp),
        colors = ButtonDefaults.buttonColors(containerColor = tokens.blue, contentColor = Color.White),
        modifier = modifier.focusRing(interaction, RoundedCornerShape(10.dp), grow = false),
        contentPadding = PaddingValues(horizontal = 20.dp, vertical = 12.dp),
    ) {
        if (icon != null) { Icon(icon, null, Modifier.size(20.dp)); Spacer(Modifier.width(8.dp)) }
        Text(text, style = MaterialTheme.typography.labelLarge, fontWeight = FontWeight.SemiBold)
    }
}

/** Pulsante secondario. */
@Composable
fun SecondaryButton(text: String, icon: androidx.compose.ui.graphics.vector.ImageVector? = null, enabled: Boolean = true, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val interaction = remember { MutableInteractionSource() }
    Button(
        onClick = onClick,
        enabled = enabled,
        interactionSource = interaction,
        shape = RoundedCornerShape(10.dp),
        colors = ButtonDefaults.buttonColors(containerColor = tokens.surface3, contentColor = tokens.ink),
        modifier = modifier.focusRing(interaction, RoundedCornerShape(10.dp), grow = false),
        contentPadding = PaddingValues(horizontal = 18.dp, vertical = 12.dp),
    ) {
        if (icon != null) { Icon(icon, null, Modifier.size(20.dp)); Spacer(Modifier.width(8.dp)) }
        Text(text, style = MaterialTheme.typography.labelLarge)
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
