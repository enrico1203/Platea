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
import androidx.compose.ui.text.font.FontWeight
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
    val ink: Color,
    val inkMuted: Color,
    val inkSubtle: Color,
    val blue: Color,
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
    line = Color(0xFF22262D), lineStrong = Color(0xFF2F343C),
    ink = Color(0xFFF3F5F7), inkMuted = Color(0xFFA2A9B4), inkSubtle = Color(0xFF808893),
    blue = Color(0xFF2F66F5), blueText = Color(0xFF6E9BFF), blueSoft = Color(0xFF0F1D3D), focus = Color(0xFF6E9BFF),
    success = Color(0xFF3CCF8E), warning = Color(0xFFF2B33D), danger = Color(0xFFFF6B6B),
    placeholder = Color(0xFF1A1D22), scrim = Color(0xB8000000),
)

val LightTokens = PlateaColors(
    bg = Color(0xFFF6F7F9), surface1 = Color(0xFFFFFFFF), surface2 = Color(0xFFEEF0F3), surface3 = Color(0xFFE4E7EC),
    line = Color(0xFFDDE1E6), lineStrong = Color(0xFFC9CED5),
    ink = Color(0xFF0A0C0F), inkMuted = Color(0xFF4A525E), inkSubtle = Color(0xFF5F6773),
    blue = Color(0xFF1F5CF0), blueText = Color(0xFF1A53D6), blueSoft = Color(0xFFE5EDFF), focus = Color(0xFF1A53D6),
    success = Color(0xFF0A7347), warning = Color(0xFF8A5300), danger = Color(0xFFB92424),
    placeholder = Color(0xFFD9DDE3), scrim = Color(0x990A0C0F),
)

val LocalPlatea = staticCompositionLocalOf { DarkTokens }

/** Colori del marchio nella composizione corrente. */
val tokens: PlateaColors @Composable get() = LocalPlatea.current

private val typography = Typography(
    displaySmall = TextStyle(fontSize = 34.sp, lineHeight = 40.sp, fontWeight = FontWeight.Bold, letterSpacing = (-0.5).sp),
    headlineMedium = TextStyle(fontSize = 26.sp, lineHeight = 32.sp, fontWeight = FontWeight.Bold, letterSpacing = (-0.3).sp),
    headlineSmall = TextStyle(fontSize = 22.sp, lineHeight = 28.sp, fontWeight = FontWeight.SemiBold),
    titleLarge = TextStyle(fontSize = 19.sp, lineHeight = 24.sp, fontWeight = FontWeight.SemiBold),
    titleMedium = TextStyle(fontSize = 16.sp, lineHeight = 22.sp, fontWeight = FontWeight.SemiBold),
    titleSmall = TextStyle(fontSize = 14.sp, lineHeight = 20.sp, fontWeight = FontWeight.SemiBold),
    bodyLarge = TextStyle(fontSize = 16.sp, lineHeight = 24.sp),
    bodyMedium = TextStyle(fontSize = 14.sp, lineHeight = 20.sp),
    bodySmall = TextStyle(fontSize = 13.sp, lineHeight = 18.sp),
    labelLarge = TextStyle(fontSize = 14.sp, lineHeight = 20.sp, fontWeight = FontWeight.Medium),
    labelMedium = TextStyle(fontSize = 12.sp, lineHeight = 16.sp, fontWeight = FontWeight.Medium),
    labelSmall = TextStyle(fontSize = 11.sp, lineHeight = 14.sp, fontWeight = FontWeight.Medium, letterSpacing = 0.4.sp),
)

@Composable
fun PlateaTheme(theme: String = "auto", forceDark: Boolean = false, content: @Composable () -> Unit) {
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
    androidx.compose.runtime.CompositionLocalProvider(LocalPlatea provides t) {
        MaterialTheme(colorScheme = scheme, typography = typography, content = content)
    }
}
