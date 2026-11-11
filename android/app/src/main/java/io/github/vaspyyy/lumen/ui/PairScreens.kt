package io.github.vaspyyy.lumen.ui

import android.content.Intent
import android.provider.Settings
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.vaspyyy.lumen.R
import io.github.vaspyyy.lumen.model.AppModel
import io.github.vaspyyy.lumen.model.Offer
import io.github.vaspyyy.lumen.model.PairStep
import io.github.vaspyyy.lumen.model.Pairing
import io.github.vaspyyy.lumen.model.PendingLink
import io.github.vaspyyy.lumen.ui.theme.Lumen
import io.github.vaspyyy.lumen.ui.theme.Mono
import kotlinx.coroutines.delay

// ------------------------------------------------------------------ type the code

@Composable
fun TypeCodeScreen(model: AppModel, onBack: () -> Unit) {
    var text by remember { mutableStateOf("") }
    var offer by remember { mutableStateOf<Offer?>(null) }
    var error by remember { mutableStateOf<String?>(null) }

    LaunchedEffect(text) {
        offer = null
        error = null
        if (text.isBlank()) return@LaunchedEffect
        delay(250)
        model.offer(text).onSuccess { offer = it }.onFailure { error = it.message }
    }

    Column(
        Modifier.fillMaxSize().background(Lumen.bg).systemBarsPadding().imePadding(),
    ) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            RoundIconButton(onBack) { Icon(Icons.back, stringResource(R.string.back), tint = Lumen.fg, modifier = Modifier.size(22.dp)) }
        }
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = 28.dp),
        ) {
            Spacer(Modifier.height(12.dp))
            Text(stringResource(R.string.type_title), style = MaterialTheme.typography.displayMedium, color = Lumen.fg)
            Spacer(Modifier.height(12.dp))
            Text(stringResource(R.string.type_body), style = MaterialTheme.typography.bodyLarge, color = Lumen.fg3)
            Spacer(Modifier.height(24.dp))
            val border by animateColorAsState(
                when {
                    offer != null -> Lumen.accent.copy(alpha = 0.7f)
                    error != null -> Lumen.pink.copy(alpha = 0.6f)
                    else -> Lumen.line2
                },
                label = "border",
            )
            Box(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(18.dp))
                    .background(Lumen.surface2)
                    .border(1.dp, border, RoundedCornerShape(18.dp))
                    .padding(start = 16.dp, end = 8.dp, top = 8.dp, bottom = 8.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.weight(1f).padding(vertical = 10.dp)) {
                        if (text.isEmpty()) {
                            Text("lumen://pair/…", style = Mono.copy(color = Lumen.fg4, fontSize = 15.sp))
                        }
                        BasicTextField(
                            value = text,
                            onValueChange = { text = it.replace("\n", "") },
                            textStyle = Mono.copy(color = Lumen.fg, fontSize = 15.sp),
                            cursorBrush = SolidColor(Lumen.accent),
                            keyboardOptions = KeyboardOptions(
                                capitalization = KeyboardCapitalization.None,
                                autoCorrectEnabled = false,
                                keyboardType = KeyboardType.Uri,
                                imeAction = ImeAction.Done,
                            ),
                            modifier = Modifier.fillMaxWidth(),
                        )
                    }
                    TextButtonLumen(stringResource(R.string.paste), { model.clipboardText()?.let { text = it.trim() } }, color = Lumen.accent)
                }
            }
            AnimatedVisibility(error != null, enter = fadeIn() + expandVertically(), exit = fadeOut() + shrinkVertically()) {
                Text(error.orEmpty(), style = MaterialTheme.typography.bodyMedium, color = Lumen.pink, modifier = Modifier.padding(top = 12.dp, start = 4.dp))
            }
            AnimatedVisibility(offer != null, enter = fadeIn() + expandVertically(), exit = fadeOut() + shrinkVertically()) {
                offer?.let { OfferSummary(it, Modifier.padding(top = 18.dp)) }
            }
        }
        PrimaryButton(
            if (offer != null) stringResource(R.string.pair_with, offer!!.label) else stringResource(R.string.pair),
            { offer?.let { model.startPairing(text.trim(), it) } },
            Modifier.fillMaxWidth().padding(horizontal = 28.dp, vertical = 12.dp),
            enabled = offer != null,
        )
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun OfferSummary(offer: Offer, modifier: Modifier = Modifier, flat: Boolean = false) {
    Column(
        if (flat) modifier.fillMaxWidth() else modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(18.dp))
            .background(Lumen.surface)
            .border(1.dp, Lumen.line, RoundedCornerShape(18.dp))
            .padding(18.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.monitor, null, tint = Lumen.accent, modifier = Modifier.size(22.dp))
            Spacer(Modifier.width(12.dp))
            Column {
                Text(offer.label, style = MaterialTheme.typography.titleMedium, color = Lumen.fg)
                Text(stringResource(R.string.as_user, offer.user), style = MaterialTheme.typography.bodySmall, color = Lumen.fg3)
            }
        }
        if (offer.addresses.isNotEmpty()) {
            Spacer(Modifier.height(14.dp))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                offer.addresses.forEach { a ->
                    Text(
                        if (offer.port != 22) "$a:${offer.port}" else a,
                        style = Mono.copy(fontSize = 12.5.sp, color = Lumen.fg2),
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .background(Lumen.surface2)
                            .padding(horizontal = 9.dp, vertical = 4.dp),
                    )
                }
            }
        }
    }
}

// ------------------------------------------------------------------ a link from outside

@Composable
fun ConfirmLink(pending: PendingLink?, onPair: () -> Unit, onDismiss: () -> Unit) {
    AnimatedVisibility(pending != null, enter = fadeIn(tween(180)), exit = fadeOut(tween(180))) {
        Box(
            Modifier
                .fillMaxSize()
                .background(Color(0xB3050409))
                .clickable(remember { MutableInteractionSource() }, null, onClick = onDismiss),
        )
    }
    Box(Modifier.fillMaxSize().navigationBarsPadding(), contentAlignment = Alignment.BottomCenter) {
        AnimatedVisibility(
            pending != null,
            enter = fadeIn() + scaleIn(spring(0.75f, Spring.StiffnessMediumLow), initialScale = 0.9f) + androidx.compose.animation.slideInVertically(spring(0.8f, Spring.StiffnessMediumLow)) { it / 3 },
            exit = fadeOut(tween(150)) + scaleOut(targetScale = 0.95f),
        ) {
            val p = pending ?: return@AnimatedVisibility
            Column(
                Modifier
                    .padding(14.dp)
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(28.dp))
                    .background(Lumen.surface2)
                    .border(1.dp, Lumen.line, RoundedCornerShape(28.dp))
                    .clickable(remember { MutableInteractionSource() }, null) {}
                    .padding(24.dp),
            ) {
                Text(
                    stringResource(R.string.confirm_link_title, p.offer.label, p.offer.user),
                    style = MaterialTheme.typography.displaySmall,
                    color = Lumen.fg,
                )
                Spacer(Modifier.height(10.dp))
                Text(stringResource(R.string.confirm_link_body), style = MaterialTheme.typography.bodyMedium, color = Lumen.fg3)
                Spacer(Modifier.height(18.dp))
                OfferSummary(p.offer, flat = true)
                Spacer(Modifier.height(22.dp))
                PrimaryButton(stringResource(R.string.pair), onPair, Modifier.fillMaxWidth())
                Spacer(Modifier.height(8.dp))
                TextButtonLumen(stringResource(R.string.not_now), onDismiss, Modifier.fillMaxWidth())
            }
        }
    }
}

// ------------------------------------------------------------------ pairing

@Composable
fun PairingScreen(p: Pairing, onRetry: () -> Unit, onBack: () -> Unit) {
    val context = LocalContext.current
    Column(Modifier.fillMaxSize().background(Lumen.bg).systemBarsPadding()) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            if (p.step != PairStep.Done) {
                RoundIconButton(onBack) { Icon(Icons.back, stringResource(R.string.back), tint = Lumen.fg, modifier = Modifier.size(22.dp)) }
            } else {
                Spacer(Modifier.size(44.dp))
            }
        }
        AnimatedContent(
            targetState = p.step == PairStep.Done,
            transitionSpec = { fadeIn(tween(400)) togetherWith fadeOut(tween(250)) },
            modifier = Modifier.weight(1f),
            label = "done",
        ) { done ->
            if (done) {
                Celebration(p.offer.label)
            } else {
                Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 28.dp)) {
                    Spacer(Modifier.height(12.dp))
                    Text(stringResource(R.string.pairing_title, p.offer.label), style = MaterialTheme.typography.displayMedium, color = Lumen.fg)
                    Spacer(Modifier.height(8.dp))
                    Text("${p.offer.user}@${p.offer.label}", style = Mono.copy(color = Lumen.fg3, fontSize = 14.sp))
                    Spacer(Modifier.height(32.dp))
                    when (p.step) {
                        PairStep.NoScreenLock -> NoScreenLock(
                            onSettings = { context.startActivity(Intent(Settings.ACTION_SECURITY_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) },
                            onRetry = onRetry,
                        )
                        else -> Steps(p)
                    }
                    AnimatedVisibility(p.step == PairStep.Failed, enter = fadeIn() + expandVertically(), exit = fadeOut() + shrinkVertically()) {
                        Column(Modifier.padding(top = 24.dp)) {
                            Text(
                                p.error,
                                style = MaterialTheme.typography.bodyLarge,
                                color = Lumen.fg,
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .clip(RoundedCornerShape(18.dp))
                                    .background(Lumen.pink.copy(alpha = 0.10f))
                                    .border(1.dp, Lumen.pink.copy(alpha = 0.35f), RoundedCornerShape(18.dp))
                                    .padding(18.dp),
                            )
                            Spacer(Modifier.height(20.dp))
                            PrimaryButton(stringResource(R.string.try_again), onRetry, Modifier.fillMaxWidth())
                            Spacer(Modifier.height(6.dp))
                            TextButtonLumen(stringResource(R.string.back), onBack, Modifier.fillMaxWidth())
                        }
                    }
                    Spacer(Modifier.height(24.dp))
                }
            }
        }
    }
}

@Composable
private fun Steps(p: Pairing) {
    val order = listOf(PairStep.Key, PairStep.Reach, PairStep.Confirm)
    val current = if (p.step == PairStep.Failed) p.failedAt else p.step
    val ci = order.indexOf(current)
    val labels = listOf(
        stringResource(R.string.step_key),
        stringResource(R.string.step_reach, p.offer.label),
        stringResource(R.string.step_confirm),
    )
    val details = listOf(
        stringResource(R.string.step_key_detail),
        p.offer.addresses.joinToString("  ·  "),
        stringResource(R.string.step_confirm_detail),
    )
    Column {
        order.forEachIndexed { i, _ ->
            val state = when {
                i < ci -> StepState.Done
                i == ci -> if (p.step == PairStep.Failed) StepState.Failed else StepState.Active
                else -> StepState.Waiting
            }
            // the PC shows the same key when it asks: that is how its owner knows this phone
            val key = p.fingerprint.takeIf { i == 2 && it.isNotEmpty() }
            StepRow(labels[i], details[i], state, last = i == order.lastIndex, key = key)
        }
    }
}

/** A key's fingerprint, set so that it can be compared with the one on the PC at a glance. */
@Composable
private fun KeyPrint(print: String) {
    val (kind, digest) = print.split(":", limit = 2).let { if (it.size == 2) it[0] + ":" to it[1] else "" to print }
    Column(
        Modifier
            .padding(top = 12.dp)
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(Lumen.surface2)
            .border(1.dp, Lumen.accent.copy(alpha = 0.28f), RoundedCornerShape(14.dp))
            .padding(horizontal = 14.dp, vertical = 12.dp),
    ) {
        Text(stringResource(R.string.step_confirm_key), style = MaterialTheme.typography.labelMedium, color = Lumen.fg3)
        Spacer(Modifier.height(6.dp))
        Text(
            buildAnnotatedString {
                withStyle(SpanStyle(color = Lumen.fg4)) { append(kind) }
                // the first characters are what people compare: they stand out
                withStyle(SpanStyle(color = Lumen.accent2)) { append(digest.take(6)) }
                append(digest.drop(6))
            },
            style = Mono.copy(color = Lumen.fg2, fontSize = 13.sp, lineHeight = 19.sp),
        )
    }
}

private enum class StepState { Waiting, Active, Done, Failed }

@Composable
private fun StepRow(title: String, detail: String, state: StepState, last: Boolean, key: String? = null) {
    // the row is as tall as its text, and the line to the next step fills what is left
    Row(Modifier.height(IntrinsicSize.Min)) {
        Column(Modifier.fillMaxHeight(), horizontalAlignment = Alignment.CenterHorizontally) {
            Box(Modifier.size(30.dp), contentAlignment = Alignment.Center) {
                AnimatedContent(state, transitionSpec = {
                    (fadeIn() + scaleIn(spring(0.5f, Spring.StiffnessMedium), initialScale = 0.6f)) togetherWith fadeOut()
                }, label = "dot") { s ->
                    when (s) {
                        StepState.Waiting -> Box(Modifier.size(10.dp).border(1.5.dp, Lumen.fg4, CircleShape))
                        StepState.Active -> CircularProgressIndicator(Modifier.size(22.dp), color = Lumen.accent, strokeWidth = 2.dp, trackColor = Lumen.accent.copy(alpha = 0.12f))
                        StepState.Done -> Box(Modifier.size(26.dp).clip(CircleShape).background(Lumen.accent.copy(alpha = 0.16f)), contentAlignment = Alignment.Center) {
                            Icon(Icons.check, null, tint = Lumen.accent, modifier = Modifier.size(16.dp))
                        }
                        StepState.Failed -> Box(Modifier.size(26.dp).clip(CircleShape).background(Lumen.pink.copy(alpha = 0.16f)), contentAlignment = Alignment.Center) {
                            Icon(Icons.close, null, tint = Lumen.pink, modifier = Modifier.size(16.dp))
                        }
                    }
                }
            }
            if (!last) Box(Modifier.padding(vertical = 4.dp).width(1.dp).weight(1f).background(Lumen.line2))
        }
        Spacer(Modifier.width(16.dp))
        Column(Modifier.padding(top = 4.dp, bottom = if (last) 0.dp else 22.dp)) {
            Text(
                title,
                style = MaterialTheme.typography.titleMedium,
                color = when (state) {
                    StepState.Waiting -> Lumen.fg4
                    StepState.Active -> Lumen.fg
                    StepState.Done -> Lumen.fg2
                    StepState.Failed -> Lumen.pink
                },
            )
            if (detail.isNotBlank() && state != StepState.Waiting) {
                Spacer(Modifier.height(4.dp))
                Text(detail, style = MaterialTheme.typography.bodyMedium, color = Lumen.fg3)
            }
            AnimatedVisibility(key != null && state == StepState.Active, enter = fadeIn() + expandVertically(), exit = fadeOut() + shrinkVertically()) {
                KeyPrint(key.orEmpty())
            }
        }
    }
}

@Composable
fun NoScreenLock(onSettings: () -> Unit, onRetry: () -> Unit) {
    Column {
        Text(stringResource(R.string.no_lock_title), style = MaterialTheme.typography.titleLarge, color = Lumen.fg)
        Spacer(Modifier.height(10.dp))
        Text(stringResource(R.string.no_lock_body), style = MaterialTheme.typography.bodyLarge, color = Lumen.fg3)
        Spacer(Modifier.height(24.dp))
        PrimaryButton(stringResource(R.string.open_security_settings), onSettings, Modifier.fillMaxWidth())
        Spacer(Modifier.height(6.dp))
        TextButtonLumen(stringResource(R.string.try_again), onRetry, Modifier.fillMaxWidth())
    }
}

/** The lamp turning on: the cursor lights up and its warmth spreads. */
@Composable
private fun Celebration(label: String) {
    val on = remember { Animatable(0f) }
    val ring = remember { Animatable(0f) }
    LaunchedEffect(Unit) {
        delay(150)
        on.animateTo(1f, spring(0.45f, Spring.StiffnessLow))
    }
    LaunchedEffect(Unit) {
        delay(250)
        ring.animateTo(1f, tween(1300, easing = FastOutSlowInEasing))
    }
    val t = rememberInfiniteTransition(label = "glow")
    val breath by t.animateFloat(0f, 1f, infiniteRepeatable(tween(2200, easing = FastOutSlowInEasing), RepeatMode.Reverse), label = "b")
    Column(Modifier.fillMaxSize().padding(horizontal = 28.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
        Box(Modifier.size(260.dp), contentAlignment = Alignment.Center) {
            Canvas(Modifier.fillMaxSize()) {
                val c = Offset(size.width / 2, size.height / 2)
                val v = on.value.coerceIn(0f, 1.2f)
                val r = size.minDimension / 2 * (0.55f + 0.45f * v) * (0.95f + 0.05f * breath)
                drawCircle(
                    Brush.radialGradient(
                        0f to Lumen.accent.copy(alpha = 0.34f * v.coerceAtMost(1f)),
                        0.5f to Lumen.accent.copy(alpha = 0.10f * v.coerceAtMost(1f)),
                        1f to Color.Transparent,
                        center = c, radius = r,
                    ),
                    radius = r, center = c,
                )
                // one soft ring travelling outwards
                val rr = size.minDimension / 2 * (0.25f + 0.75f * ring.value)
                drawCircle(Lumen.accent.copy(alpha = 0.35f * (1f - ring.value)), radius = rr, center = c, style = androidx.compose.ui.graphics.drawscope.Stroke(2.dp.toPx()))
                // the cursor itself
                val w = 34.dp.toPx()
                val h = 56.dp.toPx()
                val lit = v.coerceIn(0f, 1f)
                val off = Color(0xFF2A2440)
                val col = Color(
                    off.red + (Lumen.accent2.red - off.red) * lit,
                    off.green + (Lumen.accent2.green - off.green) * lit,
                    off.blue + (Lumen.accent2.blue - off.blue) * lit,
                )
                drawRoundRect(col, Offset(c.x - w / 2, c.y - h / 2), Size(w, h), CornerRadius(7.dp.toPx()))
            }
        }
        Spacer(Modifier.height(8.dp))
        Text(stringResource(R.string.paired_title), style = MaterialTheme.typography.displayMedium, color = Lumen.fg, modifier = Modifier.graphicsLayer { alpha = on.value.coerceIn(0f, 1f) })
        Spacer(Modifier.height(8.dp))
        Text(stringResource(R.string.paired_body, label), style = MaterialTheme.typography.bodyLarge, color = Lumen.fg3, modifier = Modifier.graphicsLayer { alpha = on.value.coerceIn(0f, 1f) })
    }
}
