package app.platea.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.unit.dp
import app.platea.R
import androidx.compose.ui.unit.sp

/** Token del marchio (`WebApp/src/styles/tokens.css`): scuro predefinito, il chiaro usa gli stessi nomi. */
@Immutable
data class PlateaColors(
    val bg: Color,
    val surface1: Color,
    val surface2: Color,
    val surface3: Color,
    val line: Color,
    val lineStrong: Color,
    val lineControl: Color,
    val ink: Color,
    val inkMuted: Color,
    val inkSubtle: Color,
    val blue: Color,
    val blueHover: Color,
    val blueText: Color,
    val blueSoft: Color,
    val focus: Color,
    val success: Color,
    val warning: Color,
    val danger: Color,
    val placeholder: Color,
    val scrim: Color,
)

val DarkTokens = PlateaColors(
    bg = Color(0xFF050608), surface1 = Color(0xFF0C0E11), surface2 = Color(0xFF14171B), surface3 = Color(0xFF1C2026),
    line = Color(0xFF22262D), lineStrong = Color(0xFF2F343C), lineControl = Color(0xFF68707C),
    ink = Color(0xFFF3F5F7), inkMuted = Color(0xFFA2A9B4), inkSubtle = Color(0xFF808893),
    blue = Color(0xFF2F66F5), blueHover = Color(0xFF2556E0), blueText = Color(0xFF6E9BFF), blueSoft = Color(0xFF0F1D3D), focus = Color(0xFF6E9BFF),
    success = Color(0xFF3CCF8E), warning = Color(0xFFF2B33D), danger = Color(0xFFFF6B6B),
    placeholder = Color(0xFF1A1D22), scrim = Color(0xB8000000),
)

val LightTokens = PlateaColors(
    bg = Color(0xFFF6F7F9), surface1 = Color(0xFFFFFFFF), surface2 = Color(0xFFEEF0F3), surface3 = Color(0xFFE4E7EC),
    line = Color(0xFFDDE1E6), lineStrong = Color(0xFFC9CED5), lineControl = Color(0xFF7F8793),
    ink = Color(0xFF0A0C0F), inkMuted = Color(0xFF4A525E), inkSubtle = Color(0xFF5F6773),
    blue = Color(0xFF1F5CF0), blueHover = Color(0xFF1A4FD2), blueText = Color(0xFF1A53D6), blueSoft = Color(0xFFE5EDFF), focus = Color(0xFF1A53D6),
    success = Color(0xFF0A7347), warning = Color(0xFF8A5300), danger = Color(0xFFB92424),
    placeholder = Color(0xFFD9DDE3), scrim = Color(0x990A0C0F),
)

val LocalPlatea = staticCompositionLocalOf { DarkTokens }

/** Colori del marchio nella composizione corrente. */
val tokens: PlateaColors @Composable get() = LocalPlatea.current

/** Geist e Geist Mono (SIL OFL), i caratteri del marchio. Pesi statici: Android 5 non legge i font variabili. */
val Geist = FontFamily(
    Font(R.font.geist_regular, FontWeight.Normal),
    Font(R.font.geist_medium, FontWeight.Medium),
    Font(R.font.geist_semibold, FontWeight.SemiBold),
    Font(R.font.geist_bold, FontWeight.Bold),
)
val GeistMono = FontFamily(
    Font(R.font.geist_mono_regular, FontWeight.Normal),
    Font(R.font.geist_mono_medium, FontWeight.Medium),
)

/**
 * Scala del marchio (`docs/brand-identity.md` › Tipografia): titoli a peso 600 con spaziatura negativa, più stretta
 * quanto più grande è il corpo. display = titolo della scheda, h1 = titolo di pagina, h2 = righe di locandine,
 * h3 = card e dialoghi, labelSmall = eyebrow (maiuscolo).
 */
private fun style(size: Int, line: Int, weight: FontWeight = FontWeight.Normal, tracking: Double = 0.0) = TextStyle(
    fontFamily = Geist, fontSize = size.sp, lineHeight = line.sp, fontWeight = weight, letterSpacing = (size * tracking).sp,
)

private val typography = Typography(
    displayLarge = style(56, 58, FontWeight.SemiBold, -0.045),
    displayMedium = style(48, 52, FontWeight.SemiBold, -0.04),
    displaySmall = style(36, 40, FontWeight.SemiBold, -0.035),
    headlineLarge = style(32, 38, FontWeight.SemiBold, -0.03),
    headlineMedium = style(28, 34, FontWeight.SemiBold, -0.03),
    headlineSmall = style(24, 30, FontWeight.SemiBold, -0.02),
    titleLarge = style(20, 26, FontWeight.SemiBold, -0.015),
    titleMedium = style(17, 24, FontWeight.SemiBold, -0.01),
    titleSmall = style(15, 20, FontWeight.SemiBold, -0.005),
    bodyLarge = style(17, 26),
    bodyMedium = style(15, 22),
    bodySmall = style(13, 18),
    labelLarge = style(14, 20, FontWeight.Medium, -0.005),
    labelMedium = style(12, 16, FontWeight.Medium),
    labelSmall = style(11, 14, FontWeight.SemiBold, 0.08),
)

/** Dati tecnici (codec, risoluzioni, bitrate, orari): Geist Mono con cifre tabellari. */
object Mono {
    val regular = TextStyle(fontFamily = GeistMono, fontSize = 13.sp, lineHeight = 18.sp, fontFeatureSettings = "tnum")
    val small = TextStyle(fontFamily = GeistMono, fontSize = 12.sp, lineHeight = 16.sp, fontWeight = FontWeight.Medium, fontFeatureSettings = "tnum")
    val numeral = TextStyle(fontFamily = GeistMono, fontSize = 15.sp, lineHeight = 20.sp, fontWeight = FontWeight.Medium, fontFeatureSettings = "tnum")
}

/** Raggi del marchio. */
object Radius {
    val xs = RoundedCornerShape(4.dp)
    val sm = RoundedCornerShape(6.dp)
    val md = RoundedCornerShape(10.dp)
    val lg = RoundedCornerShape(14.dp)
    val full = RoundedCornerShape(999.dp)
}

@Composable
fun PlateaTheme(theme: String = "dark", forceDark: Boolean = false, content: @Composable () -> Unit) {
    val dark = forceDark || when (theme) {
        "light" -> false
        "dark" -> true
        else -> isSystemInDarkTheme()
    }
    val t = if (dark) DarkTokens else LightTokens
    val scheme = if (dark) {
        darkColorScheme(
            primary = t.blue, onPrimary = Color.White, primaryContainer = t.blueSoft, onPrimaryContainer = t.blueText,
            secondary = t.blueText, onSecondary = Color.White, secondaryContainer = t.surface3, onSecondaryContainer = t.ink,
            background = t.bg, onBackground = t.ink, surface = t.bg, onSurface = t.ink,
            surfaceVariant = t.surface2, onSurfaceVariant = t.inkMuted, surfaceContainer = t.surface1,
            surfaceContainerLow = t.surface1, surfaceContainerHigh = t.surface2, surfaceContainerHighest = t.surface3,
            surfaceContainerLowest = t.bg, outline = t.lineStrong, outlineVariant = t.line, error = t.danger, onError = Color.Black,
            inverseSurface = t.ink, inverseOnSurface = t.bg,
        )
    } else {
        lightColorScheme(
            primary = t.blue, onPrimary = Color.White, primaryContainer = t.blueSoft, onPrimaryContainer = t.blueText,
            secondary = t.blueText, onSecondary = Color.White, secondaryContainer = t.surface3, onSecondaryContainer = t.ink,
            background = t.bg, onBackground = t.ink, surface = t.bg, onSurface = t.ink,
            surfaceVariant = t.surface2, onSurfaceVariant = t.inkMuted, surfaceContainer = t.surface1,
            surfaceContainerLow = t.surface1, surfaceContainerHigh = t.surface2, surfaceContainerHighest = t.surface3,
            surfaceContainerLowest = t.surface1, outline = t.lineStrong, outlineVariant = t.line, error = t.danger, onError = Color.White,
            inverseSurface = t.ink, inverseOnSurface = t.bg,
        )
    }
    // Icone della barra di stato e di navigazione leggibili sul fondo del tema (l'attività è da bordo a bordo).
    val view = androidx.compose.ui.platform.LocalView.current
    if (!view.isInEditMode) {
        androidx.compose.runtime.SideEffect {
            val window = (view.context as? android.app.Activity)?.window ?: return@SideEffect
            androidx.core.view.WindowCompat.getInsetsController(window, view).apply {
                isAppearanceLightStatusBars = !dark
                isAppearanceLightNavigationBars = !dark
            }
        }
    }
    androidx.compose.runtime.CompositionLocalProvider(LocalPlatea provides t) {
        MaterialTheme(colorScheme = scheme, typography = typography, content = content)
    }
}
