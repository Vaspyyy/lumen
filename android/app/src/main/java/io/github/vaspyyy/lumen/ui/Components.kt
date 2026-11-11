package io.github.vaspyyy.lumen.ui

import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.vaspyyy.lumen.ui.theme.Lumen
import io.github.vaspyyy.lumen.ui.theme.Mono

/** A modifier that gives a gentle spring-press scale. */
@Composable
fun Modifier.pressable(
    onClick: () -> Unit,
    enabled: Boolean = true,
    scaleTo: Float = 0.96f,
    role: Role = Role.Button,
): Modifier {
    val source = remember { MutableInteractionSource() }
    val pressed by source.collectIsPressedAsState()
    val s by animateFloatAsState(
        if (pressed) scaleTo else 1f,
        spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = Spring.StiffnessMedium),
        label = "press",
    )
    return this
        .graphicsLayer { scaleX = s; scaleY = s }
        .clickable(interactionSource = source, indication = null, enabled = enabled, role = role, onClick = onClick)
}

/** The lamp-colored button with dark ink: for the one thing to do next. */
@Composable
fun PrimaryButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, busy: Boolean = false) {
    val haptic = LocalHapticFeedback.current
    Box(
        modifier
            .defaultMinSize(minHeight = 56.dp)
            .pressable({ haptic.performHapticFeedback(HapticFeedbackType.ContextClick); onClick() }, enabled = enabled && !busy)
            .clip(RoundedCornerShape(18.dp))
            .background(if (enabled) Lumen.accent else Lumen.surface3)
            .padding(horizontal = 24.dp, vertical = 16.dp),
        contentAlignment = Alignment.Center,
    ) {
        if (busy) {
            CircularProgressIndicator(Modifier.size(20.dp), color = Lumen.ink, strokeWidth = 2.dp)
        } else {
            Text(text, style = MaterialTheme.typography.labelLarge, color = if (enabled) Lumen.ink else Lumen.fg4, textAlign = TextAlign.Center)
        }
    }
}

/** A quiet button: outlined in a hairline. */
@Composable
fun QuietButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, color: Color = Lumen.fg) {
    Box(
        modifier
            .defaultMinSize(minHeight = 52.dp)
            .pressable(onClick)
            .clip(RoundedCornerShape(18.dp))
            .border(1.dp, Lumen.line2, RoundedCornerShape(18.dp))
            .padding(horizontal = 22.dp, vertical = 14.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, style = MaterialTheme.typography.labelLarge, color = color, textAlign = TextAlign.Center)
    }
}

/** Text-only button. */
@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
@Composable
fun TextButtonLumen(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, color: Color = Lumen.fg2, onLongClick: (() -> Unit)? = null) {
    Box(
        modifier
            .defaultMinSize(minHeight = 44.dp)
            .clip(RoundedCornerShape(14.dp))
            .then(
                if (onLongClick == null) Modifier.pressable(onClick)
                else Modifier.combinedClickable(onClick = onClick, onLongClick = onLongClick),
            )
            .padding(horizontal = 14.dp, vertical = 10.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, style = MaterialTheme.typography.labelLarge, color = color)
    }
}

/** A command, shown as code. */
@Composable
fun CodeChip(text: String, modifier: Modifier = Modifier) {
    Row(
        modifier
            .clip(RoundedCornerShape(10.dp))
            .background(Lumen.surface2)
            .border(1.dp, Lumen.line, RoundedCornerShape(10.dp))
            .padding(horizontal = 10.dp, vertical = 5.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("❯ ", style = Mono.copy(fontSize = 13.sp, color = Lumen.green))
        Text(text, style = Mono.copy(fontSize = 13.sp, color = Lumen.fg))
    }
}

/** A round icon button. */
@Composable
fun RoundIconButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    size: Dp = 44.dp,
    background: Color = Lumen.surface2,
    border: Color = Lumen.line,
    content: @Composable () -> Unit,
) {
    Box(
        modifier
            .size(size)
            .pressable(onClick, scaleTo = 0.9f)
            .clip(CircleShape)
            .background(background)
            .border(1.dp, border, CircleShape),
        contentAlignment = Alignment.Center,
    ) { content() }
}

@Composable
fun Sheetish(modifier: Modifier = Modifier, padding: PaddingValues = PaddingValues(20.dp), content: @Composable RowScope.() -> Unit) {
    Row(
        modifier
            .clip(RoundedCornerShape(20.dp))
            .background(Lumen.surface2)
            .border(1.dp, Lumen.line, RoundedCornerShape(20.dp))
            .padding(padding),
        horizontalArrangement = Arrangement.spacedBy(14.dp),
        verticalAlignment = Alignment.CenterVertically,
        content = content,
    )
}
