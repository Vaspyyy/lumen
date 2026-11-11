package io.github.vaspyyy.lumen.ui

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.vaspyyy.lumen.BuildConfig
import io.github.vaspyyy.lumen.R
import io.github.vaspyyy.lumen.ui.theme.Lumen
import io.github.vaspyyy.lumen.ui.theme.Mono
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

@Composable
fun WelcomeScreen(onScan: () -> Unit, onType: () -> Unit, onDemo: () -> Unit, onFakePc: () -> Unit = {}) {
    BoxWithConstraints(Modifier.fillMaxSize().background(Lumen.bg).systemBarsPadding()) {
        val minH = maxHeight
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
            Column(
                Modifier.heightIn(min = minH).padding(horizontal = 28.dp),
                verticalArrangement = Arrangement.SpaceBetween,
            ) {
                Column {
                    Spacer(Modifier.height(20.dp))
                    Rise(0) { Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { Lamp() } }
                    Rise(1) {
                        Text(
                            stringResource(R.string.welcome_title),
                            style = MaterialTheme.typography.displayLarge,
                            color = Lumen.fg,
                        )
                    }
                    Spacer(Modifier.height(14.dp))
                    Rise(2) {
                        Text(
                            stringResource(R.string.welcome_body),
                            style = MaterialTheme.typography.bodyLarge,
                            color = Lumen.fg3,
                        )
                    }
                    Spacer(Modifier.height(34.dp))
                    Rise(3) { Step(1, stringResource(R.string.welcome_step1)) }
                    Spacer(Modifier.height(18.dp))
                    Rise(4) {
                        Step(2, stringResource(R.string.welcome_step2)) {
                            Spacer(Modifier.height(8.dp))
                            CodeChip("lumen pair")
                        }
                    }
                    Spacer(Modifier.height(18.dp))
                    Rise(5) { Step(3, stringResource(R.string.welcome_step3)) }
                    Spacer(Modifier.height(36.dp))
                }
                Rise(6) {
                    Column(Modifier.fillMaxWidth().padding(bottom = 12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        PrimaryButton(stringResource(R.string.scan_code), onScan, Modifier.fillMaxWidth())
                        TextButtonLumen(stringResource(R.string.type_code_instead), onType, Modifier.fillMaxWidth())
                        if (BuildConfig.DEBUG) {
                            TextButtonLumen(stringResource(R.string.debug_demo), onDemo, Modifier.fillMaxWidth(), color = Lumen.fg4, onLongClick = onFakePc)
                        }
                    }
                }
            }
        }
    }
}

/** Elements rise into place one after another when the screen first shows. */
@Composable
private fun Rise(index: Int, content: @Composable () -> Unit) {
    val a = remember { Animatable(0f) }
    LaunchedEffect(Unit) {
        delay(70L * index)
        a.animateTo(1f, spring(dampingRatio = 0.82f, stiffness = Spring.StiffnessLow))
    }
    Box(Modifier.graphicsLayer {
        alpha = a.value.coerceIn(0f, 1f)
        translationY = (1f - a.value) * 28.dp.toPx()
    }) { content() }
}

/** The logo with its warm light, breathing slowly. */
@Composable
fun Lamp(modifier: Modifier = Modifier, size: Int = 168) {
    val t = rememberInfiniteTransition(label = "breath")
    val breath by t.animateFloat(0f, 1f, infiniteRepeatable(tween(4200, easing = FastOutSlowInEasing), RepeatMode.Reverse), label = "b")
    val on = remember { Animatable(0f) }
    LaunchedEffect(Unit) {
        launch { on.animateTo(1f, tween(1400, easing = FastOutSlowInEasing)) }
    }
    Box(modifier.size((size * 1.5f).dp), contentAlignment = Alignment.Center) {
        Canvas(Modifier.fillMaxSize()) {
            val r = this.size.minDimension / 2f * (0.86f + 0.1f * breath)
            val a = (0.13f + 0.07f * breath) * on.value
            drawCircle(
                Brush.radialGradient(
                    0f to Lumen.accent.copy(alpha = a),
                    0.45f to Lumen.accent.copy(alpha = a * 0.35f),
                    1f to Lumen.accent.copy(alpha = 0f),
                    center = Offset(this.size.width / 2f, this.size.height / 2f),
                    radius = r,
                ),
                radius = r,
            )
        }
        Image(
            painterResource(R.drawable.logo_glow),
            contentDescription = null,
            modifier = Modifier.size((size * 1.08f).dp).graphicsLayer { alpha = 0.35f + 0.65f * on.value },
        )
    }
}

@Composable
private fun Step(n: Int, text: String, extra: @Composable () -> Unit = {}) {
    Row(verticalAlignment = Alignment.Top) {
        Box(
            Modifier
                .padding(top = 1.dp)
                .size(26.dp)
                .border(1.dp, Lumen.line2, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Text("$n", style = Mono.copy(fontSize = 12.sp, color = Lumen.accent))
        }
        Spacer(Modifier.size(14.dp))
        Column(Modifier.widthIn(max = 420.dp)) {
            Text(text, style = MaterialTheme.typography.bodyLarge, color = Lumen.fg2)
            extra()
        }
    }
}
