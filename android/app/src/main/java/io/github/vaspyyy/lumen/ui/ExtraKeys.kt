package io.github.vaspyyy.lumen.ui

import android.view.HapticFeedbackConstants
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.vaspyyy.lumen.R
import io.github.vaspyyy.lumen.term.StickyMods
import io.github.vaspyyy.lumen.ui.theme.Lumen
import io.github.vaspyyy.lumen.ui.theme.Mono
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** The row above the keyboard: the keys a phone keyboard lacks. */
@Composable
fun ExtraKeys(
    mods: StickyMods,
    onKey: (Int) -> Unit,
    onText: (String) -> Unit,
    onPaste: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = 8.dp, vertical = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Key("Esc") { onKey(4) }
        Key("Tab") { onKey(2) }
        ModKey("Ctrl", mods.ctrl) { mods.tap(true) }
        ModKey("Alt", mods.alt) { mods.tap(false) }
        Key(icon = Icons.left, label = stringResource(R.string.key_left), repeat = true) { onKey(8) }
        Key(icon = Icons.down, label = stringResource(R.string.key_down), repeat = true) { onKey(6) }
        Key(icon = Icons.up, label = stringResource(R.string.key_up), repeat = true) { onKey(5) }
        Key(icon = Icons.right, label = stringResource(R.string.key_right), repeat = true) { onKey(7) }
        Key("Home") { onKey(9) }
        Key("End") { onKey(10) }
        Key("PgUp") { onKey(11) }
        Key("PgDn") { onKey(12) }
        for (c in listOf("-", "/", "|", "~")) Key(c, mono = true) { onText(c) }
        Key(icon = Icons.paste, label = stringResource(R.string.paste)) { onPaste() }
    }
}

@Composable
private fun Key(
    text: String? = null,
    icon: ImageVector? = null,
    label: String? = null,
    mono: Boolean = false,
    repeat: Boolean = false,
    onPress: () -> Unit,
) {
    val view = LocalView.current
    var down by remember { mutableStateOf(false) }
    val scale by animateFloatAsState(if (down) 0.9f else 1f, spring(0.5f, Spring.StiffnessHigh), label = "k")
    val bg by animateColorAsState(if (down) Lumen.surface3 else Lumen.surface2, spring(stiffness = Spring.StiffnessHigh), label = "bg")
    Box(
        Modifier
            .graphicsLayer { scaleX = scale; scaleY = scale }
            .height(40.dp)
            .widthIn(min = 46.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(bg)
            .border(1.dp, Lumen.line, RoundedCornerShape(12.dp))
            .semantics {
                role = Role.Button
                contentDescription = label ?: text.orEmpty()
                // the touch handling below is its own: a screen reader presses the key through this
                onClick { onPress(); true }
            }
            .pointerInput(repeat) {
                detectTapGestures(onPress = {
                    down = true
                    view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                    onPress()
                    if (repeat) {
                        coroutineScope {
                            val job = launch {
                                delay(380)
                                while (true) {
                                    onPress()
                                    delay(55)
                                }
                            }
                            tryAwaitRelease()
                            job.cancel()
                        }
                    } else {
                        tryAwaitRelease()
                    }
                    down = false
                })
            }
            .padding(horizontal = 12.dp),
        contentAlignment = Alignment.Center,
    ) {
        if (icon != null) {
            Icon(icon, null, tint = Lumen.fg2, modifier = Modifier.size(19.dp))
        } else {
            Text(text.orEmpty(), style = Mono.copy(fontSize = if (mono) 16.sp else 13.sp, color = Lumen.fg2))
        }
    }
}

@Composable
private fun ModKey(text: String, state: StickyMods.State, onTap: () -> Unit) {
    val view = LocalView.current
    val bg by animateColorAsState(
        when (state) {
            StickyMods.State.Off -> Lumen.surface2
            StickyMods.State.Armed -> Lumen.accent.copy(alpha = 0.20f)
            StickyMods.State.Locked -> Lumen.accent
        },
        spring(stiffness = Spring.StiffnessMedium), label = "mod",
    )
    val fg = when (state) {
        StickyMods.State.Off -> Lumen.fg2
        StickyMods.State.Armed -> Lumen.accent2
        StickyMods.State.Locked -> Lumen.ink
    }
    val border = if (state == StickyMods.State.Off) Lumen.line else Lumen.accent.copy(alpha = 0.6f)
    Box(
        Modifier
            .height(40.dp)
            .widthIn(min = 52.dp)
            .pressable({
                view.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                onTap()
            }, scaleTo = 0.9f)
            .clip(RoundedCornerShape(12.dp))
            .background(bg)
            .border(1.dp, border, RoundedCornerShape(12.dp))
            .padding(horizontal = 12.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, style = Mono.copy(fontSize = 13.sp, color = fg))
        if (state == StickyMods.State.Locked) {
            Box(
                Modifier
                    .align(Alignment.BottomCenter)
                    .padding(bottom = 5.dp)
                    .size(width = 14.dp, height = 2.dp)
                    .clip(RoundedCornerShape(1.dp))
                    .background(Lumen.ink.copy(alpha = 0.7f)),
            )
        }
    }
}
