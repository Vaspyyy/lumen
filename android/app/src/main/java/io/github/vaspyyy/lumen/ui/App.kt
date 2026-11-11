package io.github.vaspyyy.lumen.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.github.vaspyyy.lumen.R
import io.github.vaspyyy.lumen.model.AppModel
import io.github.vaspyyy.lumen.model.Route
import io.github.vaspyyy.lumen.ui.theme.Lumen
import io.github.vaspyyy.lumen.ui.theme.Mono
import kotlinx.coroutines.delay

private fun depth(r: Route) = when (r) {
    Route.Welcome -> 0
    Route.Scan, Route.TypeCode -> 1
    Route.Pairing -> 2
    Route.Terminal -> 3
}

@Composable
fun LumenApp(model: AppModel) {
    var sheet by remember { mutableStateOf(false) }
    Box(Modifier.fillMaxSize().background(Lumen.bg)) {
        val err = model.coreError
        if (err != null) {
            CoreMissing(err)
        } else {
            AnimatedContent(
                targetState = model.route,
                transitionSpec = {
                    val forward = depth(targetState) >= depth(initialState)
                    val dir = if (forward) 1 else -1
                    (fadeIn(tween(260, delayMillis = 60)) +
                        slideInHorizontally(spring(0.9f, Spring.StiffnessMediumLow)) { dir * it / 6 } +
                        scaleIn(spring(0.9f, Spring.StiffnessMediumLow), initialScale = 0.98f)) togetherWith
                        (fadeOut(tween(160)) + slideOutHorizontally(spring(0.9f, Spring.StiffnessMediumLow)) { -dir * it / 10 })
                },
                label = "route",
            ) { r ->
                when (r) {
                    Route.Welcome -> WelcomeScreen(onScan = model::goScan, onType = model::goTypeCode, onDemo = model::openDemo, onFakePc = model::debugFakePc)
                    Route.Scan -> ScanScreen(model, onBack = { model.back() }, onType = model::goTypeCode)
                    Route.TypeCode -> TypeCodeScreen(model, onBack = { model.back() })
                    Route.Pairing -> model.pairing?.let { PairingScreen(it, onRetry = model::retryPairing, onBack = { model.back() }) }
                        ?: Box(Modifier.fillMaxSize().background(Lumen.bg))
                    Route.Terminal -> TerminalScreen(model, onOpenSheet = { sheet = true })
                }
            }
            BackHandler(enabled = model.route != Route.Welcome && !(model.route == Route.Terminal && model.current != null && model.conn != io.github.vaspyyy.lumen.model.Conn.Demo)) {
                model.back()
            }
            if (sheet && model.route == Route.Terminal) PcSheet(model, onDismiss = { sheet = false })
            ConfirmLink(model.pending, onPair = model::acceptPending, onDismiss = model::dismissPending)
        }
        NoticeToast(model)
    }
}

@Composable
private fun NoticeToast(model: AppModel) {
    val n = model.notice
    LaunchedEffect(n?.id) {
        if (n != null) {
            delay(2400)
            model.clearNotice(n.id)
        }
    }
    var last by remember { mutableStateOf("") }
    if (n != null) last = n.text
    Box(Modifier.fillMaxSize().navigationBarsPadding().imePadding().padding(bottom = 72.dp), contentAlignment = Alignment.BottomCenter) {
        AnimatedVisibility(
            n != null,
            enter = fadeIn(tween(150)) + slideInVertically(spring(0.7f, Spring.StiffnessMediumLow)) { it } + scaleIn(initialScale = 0.9f),
            exit = fadeOut(tween(200)) + slideOutVertically { it / 2 } + scaleOut(targetScale = 0.95f),
        ) {
            Text(
                last,
                style = MaterialTheme.typography.bodyMedium,
                color = Lumen.fg,
                modifier = Modifier
                    .padding(horizontal = 24.dp)
                    .background(Lumen.surface3, RoundedCornerShape(16.dp))
                    .border(1.dp, Lumen.line2, RoundedCornerShape(16.dp))
                    .padding(horizontal = 18.dp, vertical = 12.dp),
            )
        }
    }
}

@Composable
private fun CoreMissing(error: String) {
    Column(Modifier.fillMaxSize().systemBarsPadding().padding(28.dp)) {
        Spacer(Modifier.height(48.dp))
        Text(stringResource(R.string.core_missing_title), style = MaterialTheme.typography.displaySmall, color = Lumen.fg)
        Spacer(Modifier.height(12.dp))
        Text(stringResource(R.string.core_missing_body), style = MaterialTheme.typography.bodyLarge, color = Lumen.fg3)
        Spacer(Modifier.height(16.dp))
        Text(error, style = Mono.copy(color = Lumen.fg4))
    }
}
