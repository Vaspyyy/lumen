package io.github.vaspyyy.lumen.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.vaspyyy.lumen.BuildConfig
import io.github.vaspyyy.lumen.R
import io.github.vaspyyy.lumen.data.Pc
import io.github.vaspyyy.lumen.model.AppModel
import io.github.vaspyyy.lumen.model.Conn
import io.github.vaspyyy.lumen.ui.theme.Lumen
import io.github.vaspyyy.lumen.ui.theme.Mono
import kotlin.math.roundToInt

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PcSheet(model: AppModel, onDismiss: () -> Unit) {
    val state = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var removing by remember { mutableStateOf<Pc?>(null) }
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = state,
        containerColor = Lumen.surface2,
        scrimColor = androidx.compose.ui.graphics.Color(0x99050409),
        shape = RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp),
        dragHandle = {
            Box(Modifier.padding(top = 10.dp, bottom = 6.dp).size(width = 36.dp, height = 4.dp).clip(CircleShape).background(Lumen.line2))
        },
    ) {
        Column(
            Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp)
                .navigationBarsPadding(),
        ) {
            Spacer(Modifier.height(8.dp))
            Text(stringResource(R.string.sheet_title), style = MaterialTheme.typography.displaySmall, color = Lumen.fg)
            Spacer(Modifier.height(16.dp))
            if (model.conn == Conn.Demo) {
                Text(stringResource(R.string.sheet_demo), style = MaterialTheme.typography.bodyMedium, color = Lumen.fg3)
                Spacer(Modifier.height(12.dp))
            }
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                model.pcs.forEach { pc ->
                    val isCurrent = pc.id == model.current?.id && model.conn != Conn.Demo
                    PcRow(
                        pc = pc,
                        current = isCurrent,
                        connected = isCurrent && model.conn is Conn.Connected,
                        via = (model.conn as? Conn.Connected)?.address.orEmpty().takeIf { isCurrent }.orEmpty(),
                        onSelect = {
                            if (!isCurrent) {
                                model.switchTo(pc)
                                onDismiss()
                            }
                        },
                        onRemove = { removing = pc },
                    )
                }
            }
            Spacer(Modifier.height(12.dp))
            QuietButton(stringResource(R.string.pair_another), { onDismiss(); model.pairAnother() }, Modifier.fillMaxWidth())

            Spacer(Modifier.height(28.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(stringResource(R.string.font_size), style = MaterialTheme.typography.titleSmall, color = Lumen.fg)
                Spacer(Modifier.weight(1f))
                Text(stringResource(R.string.font_size_value, formatSize(model.fontSize)), style = Mono.copy(fontSize = 13.sp, color = Lumen.fg2))
            }
            Slider(
                value = model.fontSize,
                onValueChange = { model.setFont((it * 2).roundToInt() / 2f, save = false) },
                onValueChangeFinished = { model.setFont(model.fontSize, save = true) },
                valueRange = 7f..28f,
                colors = SliderDefaults.colors(
                    thumbColor = Lumen.accent,
                    activeTrackColor = Lumen.accent,
                    inactiveTrackColor = Lumen.line2,
                ),
            )
            Text(
                stringResource(R.string.font_preview),
                style = Mono.copy(fontSize = model.fontSize.sp, color = Lumen.fg2),
                maxLines = 1,
                overflow = TextOverflow.Clip,
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(14.dp))
                    .background(Lumen.surface)
                    .padding(horizontal = 14.dp, vertical = 12.dp),
            )
            Spacer(Modifier.height(28.dp))
            Text(
                stringResource(R.string.version_line, BuildConfig.VERSION_NAME, model.coreVersion.ifBlank { "?" }),
                style = Mono.copy(fontSize = 12.sp, color = Lumen.fg4),
            )
            Spacer(Modifier.height(20.dp))
        }
    }

    removing?.let { pc ->
        AlertDialog(
            onDismissRequest = { removing = null },
            containerColor = Lumen.surface3,
            shape = RoundedCornerShape(24.dp),
            title = { Text(stringResource(R.string.remove_title, pc.label), style = MaterialTheme.typography.titleLarge, color = Lumen.fg) },
            text = { Text(stringResource(R.string.remove_body, pc.label), style = MaterialTheme.typography.bodyMedium, color = Lumen.fg2) },
            confirmButton = {
                TextButtonLumen(stringResource(R.string.remove), {
                    removing = null
                    model.removePc(pc)
                    if (model.pcs.isEmpty()) onDismiss()
                }, color = Lumen.pink)
            },
            dismissButton = { TextButtonLumen(stringResource(R.string.keep), { removing = null }) },
        )
    }
}

@Composable
private fun PcRow(pc: Pc, current: Boolean, connected: Boolean, via: String, onSelect: () -> Unit, onRemove: () -> Unit) {
    // the address that answered while connected, else the first one the PC named
    val address = via.ifBlank { pc.addresses.firstOrNull().orEmpty() }
    Row(
        Modifier
            .fillMaxWidth()
            .pressable(onSelect, scaleTo = 0.98f)
            .clip(RoundedCornerShape(18.dp))
            .background(if (current) Lumen.surface3 else Lumen.surface)
            .border(1.dp, if (current) Lumen.line2 else Lumen.line, RoundedCornerShape(18.dp))
            .padding(start = 16.dp, top = 14.dp, bottom = 14.dp, end = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.monitor, null, tint = if (current) Lumen.accent else Lumen.fg3, modifier = Modifier.size(22.dp))
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Text(pc.label, style = MaterialTheme.typography.titleMedium, color = Lumen.fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                listOf(pc.user, address).filter { it.isNotBlank() }.joinToString(" · "),
                style = Mono.copy(fontSize = 12.sp, color = Lumen.fg3),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(Modifier.height(6.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(7.dp).clip(CircleShape).background(if (connected) Lumen.green else Lumen.fg4))
                Spacer(Modifier.width(7.dp))
                Text(
                    stringResource(if (connected) R.string.connected else R.string.not_connected),
                    style = MaterialTheme.typography.labelSmall,
                    color = if (connected) Lumen.green else Lumen.fg4,
                )
            }
        }
        Box(
            Modifier.size(44.dp).clip(CircleShape).pressable(onRemove, scaleTo = 0.85f),
            contentAlignment = Alignment.Center,
        ) {
            Icon(Icons.trash, stringResource(R.string.remove_pc, pc.label), tint = Lumen.fg3, modifier = Modifier.size(20.dp))
        }
    }
}
