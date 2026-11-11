package io.github.vaspyyy.lumen.ui.theme

import androidx.compose.foundation.text.selection.TextSelectionColors
import androidx.compose.foundation.text.selection.LocalTextSelectionColors
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import io.github.vaspyyy.lumen.R

/** lumen's colors: a night sky with a warm lamp in it. */
object Lumen {
    val bg = Color(0xFF0B0A11)
    val surface = Color(0xFF12111A)
    val surface2 = Color(0xFF171522)
    val surface3 = Color(0xFF1F1C2C)
    val line = Color(0x14FFFFFF)
    val line2 = Color(0x24FFFFFF)

    val fg = Color(0xFFECE9F4)
    val fg2 = Color(0xFFBDB8CF)
    val fg3 = Color(0xFF938EA8)
    val fg4 = Color(0xFF8782A0)

    val accent = Color(0xFFFFC97A)
    val accent2 = Color(0xFFFFD9A0)
    val ink = Color(0xFF1C1305)
    val violet = Color(0xFFC9A4FF)
    val blue = Color(0xFF8CB4FF)
    val green = Color(0xFF8FDFA8)
    val pink = Color(0xFFF27A93)
    val cyan = Color(0xFF7FD9E6)
}

private fun geist(weight: Int) = Font(
    R.font.geist,
    weight = FontWeight(weight),
    variationSettings = FontVariation.Settings(FontVariation.weight(weight)),
)

val Geist = FontFamily(geist(400), geist(500), geist(600), geist(700))
val InstrumentSerif = FontFamily(Font(R.font.instrument_serif))
val MapleMono = FontFamily(
    Font(R.font.maple_mono_regular),
    Font(R.font.maple_mono_bold, FontWeight.Bold),
    Font(R.font.maple_mono_italic, style = FontStyle.Italic),
    Font(R.font.maple_mono_bold_italic, FontWeight.Bold, FontStyle.Italic),
)

/** Monospace text without Maple Mono's ligatures. */
val Mono = TextStyle(fontFamily = MapleMono, fontFeatureSettings = "liga 0, calt 0", fontSize = 14.sp)

private val typography = Typography().let { t ->
    fun TextStyle.g() = copy(fontFamily = Geist)
    Typography(
        displayLarge = TextStyle(fontFamily = InstrumentSerif, fontSize = 46.sp, lineHeight = 50.sp, letterSpacing = (-0.01).em),
        displayMedium = TextStyle(fontFamily = InstrumentSerif, fontSize = 38.sp, lineHeight = 42.sp),
        displaySmall = TextStyle(fontFamily = InstrumentSerif, fontSize = 30.sp, lineHeight = 34.sp),
        headlineLarge = t.headlineLarge.g(),
        headlineMedium = t.headlineMedium.g(),
        headlineSmall = t.headlineSmall.g(),
        titleLarge = TextStyle(fontFamily = Geist, fontWeight = FontWeight.SemiBold, fontSize = 20.sp, lineHeight = 26.sp, letterSpacing = (-0.01).em),
        titleMedium = TextStyle(fontFamily = Geist, fontWeight = FontWeight.Medium, fontSize = 16.sp, lineHeight = 22.sp),
        titleSmall = TextStyle(fontFamily = Geist, fontWeight = FontWeight.Medium, fontSize = 14.sp, lineHeight = 20.sp),
        bodyLarge = TextStyle(fontFamily = Geist, fontSize = 16.sp, lineHeight = 24.sp),
        bodyMedium = TextStyle(fontFamily = Geist, fontSize = 14.sp, lineHeight = 21.sp),
        bodySmall = TextStyle(fontFamily = Geist, fontSize = 12.5.sp, lineHeight = 18.sp),
        labelLarge = TextStyle(fontFamily = Geist, fontWeight = FontWeight.Medium, fontSize = 15.sp, lineHeight = 20.sp),
        labelMedium = TextStyle(fontFamily = Geist, fontWeight = FontWeight.Medium, fontSize = 13.sp, lineHeight = 18.sp),
        labelSmall = TextStyle(fontFamily = Geist, fontWeight = FontWeight.Medium, fontSize = 11.5.sp, lineHeight = 16.sp, letterSpacing = 0.02.em),
    )
}

private val scheme = darkColorScheme(
    primary = Lumen.accent,
    onPrimary = Lumen.ink,
    primaryContainer = Lumen.surface3,
    onPrimaryContainer = Lumen.fg,
    secondary = Lumen.violet,
    onSecondary = Lumen.ink,
    tertiary = Lumen.blue,
    background = Lumen.bg,
    onBackground = Lumen.fg,
    surface = Lumen.surface,
    onSurface = Lumen.fg,
    surfaceVariant = Lumen.surface2,
    onSurfaceVariant = Lumen.fg2,
    surfaceContainerLowest = Lumen.bg,
    surfaceContainerLow = Lumen.surface,
    surfaceContainer = Lumen.surface2,
    surfaceContainerHigh = Lumen.surface2,
    surfaceContainerHighest = Lumen.surface3,
    outline = Lumen.line2,
    outlineVariant = Lumen.line,
    error = Lumen.pink,
    onError = Lumen.ink,
    scrim = Color(0xB3050409),
)

@Composable
fun LumenTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = scheme, typography = typography) {
        CompositionLocalProvider(
            LocalTextSelectionColors provides TextSelectionColors(Lumen.accent, Lumen.accent.copy(alpha = 0.32f)),
            content = content,
        )
    }
}
