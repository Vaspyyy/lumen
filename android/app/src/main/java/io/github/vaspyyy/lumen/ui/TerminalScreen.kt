package io.github.vaspyyy.lumen.ui

import android.content.Intent
import androidx.core.net.toUri
import android.provider.Settings
import android.view.HapticFeedbackConstants
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandHorizontally
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.shrinkHorizontally
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.displayCutout
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.union
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import io.github.vaspyyy.lumen.R
import io.github.vaspyyy.lumen.model.AppModel
import io.github.vaspyyy.lumen.model.Conn
import io.github.vaspyyy.lumen.model.Grid
import io.github.vaspyyy.lumen.model.Kept
import io.github.vaspyyy.lumen.model.Tab
import io.github.vaspyyy.lumen.term.StickyMods
import io.github.vaspyyy.lumen.term.TerminalView
import io.github.vaspyyy.lumen.ui.theme.Lumen
import io.github.vaspyyy.lumen.ui.theme.Mono
import kotlinx.coroutines.delay
import kotlin.math.roundToInt

@Composable
fun TerminalScreen(model: AppModel, onOpenSheet: () -> Unit) {
    val context = LocalContext.current
    val mods = remember { StickyMods() }
    var view by remember { mutableStateOf<TerminalView?>(null) }
    var scrolledBack by remember { mutableStateOf(false) }
    var selection by remember { mutableStateOf<IntOffset?>(null) }
    var pinch by remember { mutableStateOf<Float?>(null) }
    var link by remember { mutableStateOf<TappedLink?>(null) }
    val activeTab = model.tabs.getOrNull(model.active)
    val conn = model.conn
    val live = conn is Conn.Connected || conn == Conn.Demo

    val host = remember(model) {
        object : TerminalView.Host {
            override fun onGrid(cols: Int, rows: Int, cellWidth: Int, cellHeight: Int) = model.onGrid(Grid(cols, rows, cellWidth, cellHeight))
            override fun onScrolledBack(back: Boolean) { scrolledBack = back }
            override fun onSelection(shown: Boolean, x: Float, y: Float) {
                selection = if (shown) IntOffset(x.roundToInt(), y.roundToInt()) else null
            }
            override fun onFontSize(size: Float, done: Boolean) {
                model.setFont(size, save = done)
                pinch = if (done) null else size
            }
            override fun onLink(target: String?, isFile: Boolean) {
                link = target?.let { TappedLink(it, isFile) }
            }
            override fun copy(text: String) { model.copy(text) }
            override fun clipboard(): String? = model.clipboardText()
        }
    }

    // Held sideways, the phone has its camera and maybe its navigation bar at
    // the sides: nothing to read or to touch goes under them.
    val sides = WindowInsets.displayCutout.union(WindowInsets.navigationBars).only(WindowInsetsSides.Horizontal)
    Column(Modifier.fillMaxSize().background(Lumen.bg).windowInsetsPadding(sides)) {
        TopBar(model, onOpenSheet)
        Box(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .clip(RoundedCornerShape(topStart = 20.dp, topEnd = 20.dp))
                .background(Lumen.surface),
        ) {
            AndroidView(
                factory = { ctx ->
                    TerminalView(ctx, mods).also {
                        it.host = host
                        it.setFont(model.fontSize)
                        view = it
                    }
                },
                update = { v ->
                    if (v.fontSize != model.fontSize && pinch == null) v.setFont(model.fontSize)
                    v.bind(activeTab)
                },
                modifier = Modifier.fillMaxSize(),
            )
            if (activeTab != null) {
                // keep focus on the terminal so hardware keys reach it
                LaunchedEffect(activeTab.id) { view?.requestFocus() }
            }

            // the last screen stays, dimmed, while the connection is not there
            val dim by animateFloatAsState(if (!live && model.tabs.isNotEmpty()) 1f else 0f, tween(300), label = "dim")
            if (dim > 0f) {
                Box(Modifier.fillMaxSize().graphicsLayer { alpha = dim }.background(Color(0x9E0B0A11)))
            }

            if (model.tabs.isEmpty()) {
                EmptyState(model)
            } else if (!live) {
                StatusPill(model, Modifier.align(Alignment.TopCenter).padding(top = 14.dp))
            }

            // the bar for a selection, above where it starts
            val density = LocalDensity.current
            androidx.compose.animation.AnimatedVisibility(
                selection != null,
                enter = fadeIn(tween(120)) + scaleIn(spring(0.6f, Spring.StiffnessMedium), initialScale = 0.85f),
                exit = fadeOut(tween(100)) + scaleOut(targetScale = 0.9f),
                modifier = Modifier.offset {
                    val p = selection ?: IntOffset.Zero
                    val w = with(density) { 190.dp.roundToPx() }
                    val maxX = with(density) { 400.dp.roundToPx() }
                    IntOffset((p.x - w / 2).coerceIn(12, maxX), (p.y - with(density) { 62.dp.roundToPx() }).coerceAtLeast(8))
                },
            ) {
                SelectionBar(onCopy = { view?.copySelection() }, onPaste = { view?.paste() })
            }

            androidx.compose.animation.AnimatedVisibility(
                scrolledBack && live,
                enter = fadeIn() + scaleIn(spring(0.55f, Spring.StiffnessMediumLow), initialScale = 0.6f),
                exit = fadeOut() + scaleOut(targetScale = 0.6f),
                modifier = Modifier.align(Alignment.BottomEnd).padding(18.dp),
            ) {
                RoundIconButton({ view?.scrollToBottom() }, size = 48.dp, background = Lumen.surface3, border = Lumen.line2) {
                    Icon(Icons.toBottom, stringResource(R.string.to_bottom), tint = Lumen.accent, modifier = Modifier.size(22.dp))
                }
            }

            // A link that was tapped says where it leads before anything is done with it.
            LaunchedEffect(activeTab?.id) { link = null }
            androidx.compose.animation.AnimatedVisibility(
                link != null && live,
                enter = fadeIn(tween(120)) + slideInVertically(spring(0.75f, Spring.StiffnessMedium)) { it / 2 },
                exit = fadeOut(tween(120)) + slideOutVertically { it / 2 },
                modifier = Modifier.align(Alignment.BottomCenter).padding(12.dp),
            ) {
                var shown by remember { mutableStateOf(link) }
                link?.let { shown = it }
                val noApp = stringResource(R.string.link_no_app)
                shown?.let { l ->
                    LinkBar(
                        l,
                        onOpen = {
                            link = null
                            runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, l.target.toUri()).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
                                .onFailure { model.notify(noApp) }
                        },
                        onCopy = {
                            link = null
                            model.copy(l.target)
                        },
                        onDismiss = { link = null },
                    )
                }
            }

            androidx.compose.animation.AnimatedVisibility(
                pinch != null,
                enter = fadeIn(tween(100)) + scaleIn(initialScale = 0.9f),
                exit = fadeOut(tween(250)),
                modifier = Modifier.align(Alignment.Center),
            ) {
                val size = pinch ?: model.fontSize
                Text(
                    stringResource(R.string.font_size_value, formatSize(size)),
                    style = Mono.copy(fontSize = 15.sp, color = Lumen.fg),
                    modifier = Modifier
                        .clip(RoundedCornerShape(16.dp))
                        .background(Color(0xE61F1C2C))
                        .border(1.dp, Lumen.line2, RoundedCornerShape(16.dp))
                        .padding(horizontal = 18.dp, vertical = 10.dp),
                )
            }
        }
        Box(
            Modifier
                .fillMaxWidth()
                .background(Lumen.surface)
                .windowInsetsPadding(WindowInsets.navigationBars.union(WindowInsets.ime)),
        ) {
            if (model.tabs.isNotEmpty()) ExtraKeys(
                mods = mods,
                onKey = { view?.pressKey(it) },
                onText = { view?.pressText(it) },
                onPaste = { view?.paste() },
            )
        }
    }
}

/** A link in the terminal that was tapped: where it leads, and whether that is a file on the PC. */
data class TappedLink(val target: String, val isFile: Boolean) {
    /** Web pages are opened; of anything else, the phone can only copy where it leads. */
    val opens: Boolean get() = !isFile && (target.startsWith("http://", true) || target.startsWith("https://", true))
}

/** Where a tapped link leads, in full, and what can be done with it. */
@Composable
private fun LinkBar(link: TappedLink, onOpen: () -> Unit, onCopy: () -> Unit, onDismiss: () -> Unit) {
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(18.dp))
            .background(Color(0xF21F1C2C))
            .border(1.dp, Lumen.line2, RoundedCornerShape(18.dp))
            .padding(start = 16.dp, end = 6.dp, top = 6.dp, bottom = 8.dp),
    ) {
        Row(verticalAlignment = Alignment.Top) {
            Text(
                linkText(link),
                style = Mono.copy(fontSize = 12.5.sp, lineHeight = 18.sp),
                maxLines = 4,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f).padding(top = 10.dp, bottom = 6.dp),
            )
            val close = stringResource(R.string.dismiss)
            Box(
                Modifier.size(40.dp).clip(CircleShape).pressable(onDismiss).semantics { contentDescription = close },
                contentAlignment = Alignment.Center,
            ) {
                Icon(Icons.close, null, tint = Lumen.fg3, modifier = Modifier.size(16.dp))
            }
        }
        Row(Modifier.fillMaxWidth().padding(end = 6.dp), horizontalArrangement = Arrangement.End, verticalAlignment = Alignment.CenterVertically) {
            if (link.opens) {
                BarButton(Icons.copy, stringResource(R.string.copy), onCopy)
                Spacer(Modifier.width(6.dp))
            }
            Box(
                Modifier
                    .clip(RoundedCornerShape(13.dp))
                    .pressable(if (link.opens) onOpen else onCopy)
                    .background(Lumen.accent)
                    .padding(horizontal = 16.dp, vertical = 9.dp),
            ) {
                val label = when {
                    link.opens -> R.string.link_open
                    link.isFile -> R.string.link_copy_path
                    else -> R.string.copy
                }
                Text(stringResource(label), style = MaterialTheme.typography.labelMedium, color = Lumen.ink)
            }
        }
    }
}

/** A web address with the machine it leads to standing out: that part says whose page it is. */
private fun linkText(link: TappedLink): AnnotatedString = buildAnnotatedString {
    val t = link.target
    val from = t.indexOf("://").takeIf { link.opens && it >= 0 }?.plus(3)
    if (from == null) {
        withStyle(SpanStyle(color = Lumen.fg)) { append(t) }
        return@buildAnnotatedString
    }
    val to = t.indexOfAny(charArrayOf('/', '?', '#'), from).takeIf { it >= 0 } ?: t.length
    withStyle(SpanStyle(color = Lumen.fg3)) { append(t.substring(0, from)) }
    withStyle(SpanStyle(color = Lumen.accent)) { append(t.substring(from, to)) }
    withStyle(SpanStyle(color = Lumen.fg3)) { append(t.substring(to)) }
}

fun formatSize(s: Float): String = if (s % 1f == 0f) s.toInt().toString() else "%.1f".format(s)

// ------------------------------------------------------------------ top bar

@Composable
private fun TopBar(model: AppModel, onOpenSheet: () -> Unit) {
    val listState = rememberLazyListState()
    // the active tab stays in view, also when its title grows
    LaunchedEffect(model.active, model.tabs.size, model.tabs.getOrNull(model.active)?.label) {
        if (model.tabs.isNotEmpty()) listState.animateScrollToItem(model.active.coerceIn(0, model.tabs.lastIndex))
    }
    Row(
        Modifier
            .fillMaxWidth()
            .statusBarsPadding()
            .padding(start = 8.dp, end = 8.dp, top = 4.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        LazyRow(
            state = listState,
            // tabs that run on under an edge fade out there instead of being cut
            modifier = Modifier
                .weight(1f)
                .graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
                .drawWithContent {
                    drawContent()
                    val fade = 18.dp.toPx()
                    if (listState.canScrollBackward) {
                        drawRect(Brush.horizontalGradient(listOf(Color.Transparent, Color.Black), 0f, fade), size = Size(fade, size.height), blendMode = BlendMode.DstIn)
                    }
                    if (listState.canScrollForward) {
                        drawRect(
                            Brush.horizontalGradient(listOf(Color.Black, Color.Transparent), size.width - fade, size.width),
                            topLeft = Offset(size.width - fade, 0f),
                            size = Size(fade, size.height),
                            blendMode = BlendMode.DstIn,
                        )
                    }
                },
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            contentPadding = PaddingValues(horizontal = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            itemsIndexed(model.tabs, key = { _, t -> t.id }) { i, t ->
                TabPill(
                    tab = t,
                    active = i == model.active,
                    onSelect = { model.select(i) },
                    onClose = { model.closeTab(t, keepRunning = false) },
                    onKeep = { model.closeTab(t, keepRunning = true) },
                    canKeep = model.conn.let { it is Conn.Connected && it.keeps },
                    modifier = Modifier.animateItem(fadeInSpec = spring(stiffness = Spring.StiffnessMediumLow), placementSpec = spring(0.8f, Spring.StiffnessMediumLow)),
                )
            }
        }
        val canAdd = model.conn.let { it is Conn.Connected || it == Conn.Demo }
        androidx.compose.animation.AnimatedVisibility(canAdd, enter = fadeIn() + scaleIn(initialScale = 0.7f), exit = fadeOut() + scaleOut(targetScale = 0.7f)) {
            RoundIconButton({ model.newTab() }, size = 38.dp, background = Color.Transparent, border = Lumen.line) {
                Icon(Icons.plus, stringResource(R.string.new_terminal), tint = Lumen.fg2, modifier = Modifier.size(20.dp))
            }
        }
        Spacer(Modifier.width(4.dp))
        StatusDot(model.conn, onOpenSheet)
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun TabPill(
    tab: Tab,
    active: Boolean,
    onSelect: () -> Unit,
    onClose: () -> Unit,
    onKeep: () -> Unit,
    canKeep: Boolean,
    modifier: Modifier = Modifier,
) {
    val view = LocalView.current
    var menu by remember { mutableStateOf(false) }
    val flash = remember { Animatable(0f) }
    LaunchedEffect(tab.bells) {
        if (tab.bells > 0) {
            view.performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
            flash.snapTo(1f)
            flash.animateTo(0f, tween(700, easing = FastOutSlowInEasing))
        }
    }
    val bg by animateColorAsState(if (active) Lumen.surface3 else Color.Transparent, spring(stiffness = Spring.StiffnessMediumLow), label = "pill")
    val fg by animateColorAsState(if (active) Lumen.fg else Lumen.fg3, label = "pillfg")
    val dot by animateFloatAsState(if (active) 1f else 0f, spring(0.6f, Spring.StiffnessMediumLow), label = "dot")
    Box(modifier) {
        Row(
            Modifier
                .height(36.dp)
                .clip(RoundedCornerShape(14.dp))
                .drawBehind {
                    drawRect(bg)
                    if (flash.value > 0f) drawRect(Lumen.accent.copy(alpha = 0.35f * flash.value))
                }
                .border(1.dp, if (active) Lumen.line2 else Lumen.line, RoundedCornerShape(14.dp))
                .combinedClickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                    onClick = onSelect,
                    onLongClick = {
                        view.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
                        menu = true
                    },
                )
                .padding(start = 12.dp, end = if (active) 4.dp else 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // the lamp: lit on the active tab
            Box(
                Modifier
                    .size((7 * dot).dp)
                    .graphicsLayer { alpha = dot }
                    .drawBehind {
                        drawCircle(Brush.radialGradient(listOf(Lumen.accent.copy(alpha = 0.5f), Color.Transparent)), radius = size.minDimension * 1.4f)
                    }
                    .clip(CircleShape)
                    .background(Lumen.accent),
            )
            if (dot > 0.01f) Spacer(Modifier.width((8 * dot).dp))
            Text(
                tab.label,
                style = Mono.copy(fontSize = 12.5.sp, color = fg),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.widthIn(max = 168.dp),
            )
            AnimatedVisibility(active, enter = fadeIn() + expandHorizontally(), exit = fadeOut() + shrinkHorizontally()) {
                Box(
                    Modifier
                        .padding(start = 4.dp)
                        .size(28.dp)
                        .clip(CircleShape)
                        .pressable(onClose, scaleTo = 0.85f),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(Icons.close, stringResource(R.string.close_tab), tint = Lumen.fg3, modifier = Modifier.size(16.dp))
                }
            }
        }
        DropdownMenu(
            expanded = menu,
            onDismissRequest = { menu = false },
            containerColor = Lumen.surface3,
            shape = RoundedCornerShape(16.dp),
        ) {
            DropdownMenuItem(
                text = { Text(stringResource(if (tab.demo) R.string.close_tab else R.string.close_and_end), color = Lumen.fg) },
                onClick = { menu = false; onClose() },
            )
            if (canKeep) {
                DropdownMenuItem(
                    text = { Text(stringResource(R.string.keep_running), color = Lumen.fg) },
                    onClick = { menu = false; onKeep() },
                )
            }
        }
    }
}

@Composable
private fun StatusDot(conn: Conn, onClick: () -> Unit) {
    val color = when (conn) {
        is Conn.Connected -> Lumen.green
        Conn.Demo -> Lumen.violet
        Conn.Idle -> Lumen.fg4
        Conn.Checking, Conn.Auth, is Conn.Connecting, is Conn.Reconnecting -> Lumen.accent
        else -> Lumen.pink
    }
    val busy = conn == Conn.Checking || conn == Conn.Auth || conn is Conn.Connecting || conn is Conn.Reconnecting
    // The pulse runs only while something is under way: an animation that never
    // ends would redraw the screen on every frame, for nothing.
    val pulse = if (busy) {
        val t = rememberInfiniteTransition(label = "pulse")
        t.animateFloat(0.35f, 1f, infiniteRepeatable(tween(800, easing = FastOutSlowInEasing), RepeatMode.Reverse), label = "p").value
    } else {
        1f
    }
    val desc = stringResource(R.string.connection_and_pcs)
    Box(
        Modifier
            .size(40.dp)
            .clip(CircleShape)
            .pressable(onClick, scaleTo = 0.85f)
            .semantics { contentDescription = desc },
        contentAlignment = Alignment.Center,
    ) {
        val c by animateColorAsState(color, label = "dotc")
        Box(
            Modifier
                .size(22.dp)
                .drawBehind {
                    drawCircle(Brush.radialGradient(listOf(c.copy(alpha = 0.45f * pulse), Color.Transparent)))
                },
        )
        Box(Modifier.size(9.dp).graphicsLayer { alpha = pulse }.clip(CircleShape).background(c))
    }
}

// ------------------------------------------------------------------ states

/** What the connection is doing, in words, and what can be done about it. */
@Composable
private fun connText(conn: Conn, label: String): String = when (conn) {
    Conn.Idle -> stringResource(R.string.conn_idle, label)
    Conn.Demo -> stringResource(R.string.conn_demo)
    Conn.Checking -> stringResource(R.string.conn_checking)
    Conn.Auth -> stringResource(R.string.conn_auth)
    is Conn.Connecting -> stringResource(R.string.conn_connecting, label)
    is Conn.Connected -> stringResource(R.string.conn_connected, label)
    is Conn.Reconnecting -> stringResource(R.string.conn_reconnecting, conn.seconds)
    is Conn.Failed -> conn.message
    Conn.AuthCancelled -> stringResource(R.string.conn_auth_cancelled)
    Conn.Refused -> stringResource(R.string.conn_refused)
    Conn.WrongHost -> stringResource(R.string.conn_wrong_host)
    Conn.KeyGone -> stringResource(R.string.conn_key_gone)
    Conn.NoScreenLock -> stringResource(R.string.conn_no_lock)
}

@Composable
private fun StatusPill(model: AppModel, modifier: Modifier = Modifier) {
    val conn = model.conn
    val label = model.current?.label.orEmpty()
    val busy = conn == Conn.Checking || conn == Conn.Auth || conn is Conn.Connecting || conn is Conn.Reconnecting
    val retry = conn is Conn.Failed || conn == Conn.AuthCancelled
    if (!busy && !retry) {
        // the bigger problems get the whole screen
        ConnProblem(model, Modifier.fillMaxSize())
        return
    }
    Row(
        modifier
            .padding(horizontal = 16.dp)
            .clip(RoundedCornerShape(18.dp))
            .background(Color(0xF21F1C2C))
            .border(1.dp, Lumen.line2, RoundedCornerShape(18.dp))
            .padding(start = 14.dp, end = if (retry) 6.dp else 16.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (busy) {
            CircularProgressIndicator(Modifier.size(14.dp), color = Lumen.accent, strokeWidth = 1.6.dp)
            Spacer(Modifier.width(10.dp))
        }
        Text(
            connText(conn, label),
            style = MaterialTheme.typography.bodyMedium,
            color = Lumen.fg,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false).padding(vertical = 6.dp),
        )
        if (retry) {
            Spacer(Modifier.width(8.dp))
            Box(
                Modifier
                    .clip(RoundedCornerShape(13.dp))
                    .pressable({ model.retry() })
                    .background(Lumen.accent)
                    .padding(horizontal = 14.dp, vertical = 8.dp),
            ) {
                Text(stringResource(R.string.try_again), style = MaterialTheme.typography.labelMedium, color = Lumen.ink)
            }
        }
    }
}

@Composable
private fun EmptyState(model: AppModel) {
    val conn = model.conn
    val label = model.current?.label.orEmpty()
    if (conn !is Conn.Connected && conn != Conn.Demo) {
        ConnProblem(model, Modifier.fillMaxSize())
        return
    }
    LaunchedEffect(Unit) { model.refreshKept() }
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(28.dp),
        verticalArrangement = Arrangement.Center,
    ) {
        Spacer(Modifier.height(24.dp))
        Text(stringResource(R.string.empty_title), style = MaterialTheme.typography.displaySmall, color = Lumen.fg)
        Spacer(Modifier.height(8.dp))
        Text(stringResource(R.string.empty_body, label), style = MaterialTheme.typography.bodyLarge, color = Lumen.fg3)
        Spacer(Modifier.height(24.dp))
        PrimaryButton(stringResource(R.string.new_terminal), { model.newTab() }, Modifier.fillMaxWidth())
        if (model.kept.isNotEmpty()) {
            Spacer(Modifier.height(32.dp))
            Text(stringResource(R.string.running_on_pc, label), style = MaterialTheme.typography.labelMedium, color = Lumen.fg3)
            Spacer(Modifier.height(10.dp))
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                model.kept.forEach { k -> KeptRow(k) { model.openKept(k) } }
            }
        }
    }
}

/** A terminal the PC keeps that is not open here, to open with a tap. */
@Composable
fun KeptRow(k: Kept, onOpen: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .pressable(onOpen, scaleTo = 0.98f)
            .clip(RoundedCornerShape(16.dp))
            .background(Lumen.surface2)
            .border(1.dp, Lumen.line, RoundedCornerShape(16.dp))
            .padding(16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.prompt, null, tint = Lumen.accent, modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Text(k.name, style = Mono.copy(fontSize = 14.sp, color = Lumen.fg))
            val sub = listOf(k.title, k.cwd).filter { it.isNotBlank() }.distinct().joinToString("  ·  ")
            if (sub.isNotEmpty()) {
                Text(sub, style = Mono.copy(fontSize = 12.sp, color = Lumen.fg3), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        Text(stringResource(R.string.open), style = MaterialTheme.typography.labelMedium, color = Lumen.accent)
    }
}

/** The connection, when it is not there and there is no screen to show under it. */
@Composable
private fun ConnProblem(model: AppModel, modifier: Modifier = Modifier) {
    val conn = model.conn
    val pc = model.current
    val label = pc?.label.orEmpty()
    val context = LocalContext.current
    Column(modifier.verticalScroll(rememberScrollState()).padding(28.dp), verticalArrangement = Arrangement.Center) {
        AnimatedContent(conn, contentKey = { it::class }, transitionSpec = {
            (fadeIn(tween(220)) + slideInVertically(spring(0.85f, Spring.StiffnessMediumLow)) { it / 8 }) togetherWith
                (fadeOut(tween(150)) + slideOutVertically { -it / 8 })
        }, label = "conn") { c ->
            val shownBusy = c == Conn.Checking || c == Conn.Auth || c is Conn.Connecting || c is Conn.Reconnecting
            Column {
                if (shownBusy) {
                    ConnectingLamp()
                    Spacer(Modifier.height(24.dp))
                }
                Text(
                    when (c) {
                        Conn.Refused -> stringResource(R.string.refused_title)
                        Conn.WrongHost -> stringResource(R.string.wrong_host_title)
                        Conn.KeyGone -> stringResource(R.string.key_gone_title)
                        Conn.NoScreenLock -> stringResource(R.string.no_lock_title)
                        is Conn.Failed -> stringResource(R.string.failed_title, label)
                        Conn.AuthCancelled -> stringResource(R.string.auth_cancelled_title)
                        else -> label
                    },
                    style = MaterialTheme.typography.displaySmall,
                    color = Lumen.fg,
                )
                Spacer(Modifier.height(10.dp))
                Text(
                    when (c) {
                        Conn.Refused -> stringResource(R.string.refused_body, label)
                        Conn.WrongHost -> stringResource(R.string.wrong_host_body, label)
                        Conn.KeyGone -> stringResource(R.string.key_gone_body, label)
                        Conn.NoScreenLock -> stringResource(R.string.no_lock_body)
                        else -> connText(c, label)
                    },
                    style = MaterialTheme.typography.bodyLarge,
                    color = Lumen.fg3,
                )
                Spacer(Modifier.height(24.dp))
                when (c) {
                    is Conn.Failed, Conn.AuthCancelled, Conn.Idle -> PrimaryButton(stringResource(R.string.try_again), { model.retry() }, Modifier.fillMaxWidth())
                    Conn.Refused, Conn.KeyGone -> {
                        PrimaryButton(stringResource(R.string.pair_again), { model.pairAnother() }, Modifier.fillMaxWidth())
                        Spacer(Modifier.height(6.dp))
                        if (pc != null) TextButtonLumen(stringResource(R.string.remove_pc, label), { model.removePc(pc) }, Modifier.fillMaxWidth(), color = Lumen.pink)
                    }
                    Conn.NoScreenLock -> {
                        PrimaryButton(stringResource(R.string.open_security_settings), {
                            context.startActivity(Intent(Settings.ACTION_SECURITY_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                        }, Modifier.fillMaxWidth())
                        Spacer(Modifier.height(6.dp))
                        TextButtonLumen(stringResource(R.string.try_again), { model.retry() }, Modifier.fillMaxWidth())
                    }
                    Conn.WrongHost -> {
                        if (pc != null) QuietButton(stringResource(R.string.remove_pc, label), { model.removePc(pc) }, Modifier.fillMaxWidth(), color = Lumen.pink)
                    }
                    else -> Unit
                }
            }
        }
    }
}

/** The cursor, waiting to light up. */
@Composable
private fun ConnectingLamp() {
    val t = rememberInfiniteTransition(label = "lamp")
    val a by t.animateFloat(0.25f, 1f, infiniteRepeatable(tween(900, easing = FastOutSlowInEasing), RepeatMode.Reverse), label = "a")
    Box(Modifier.size(64.dp), contentAlignment = Alignment.CenterStart) {
        Box(
            Modifier
                .size(64.dp)
                .drawBehind {
                    drawCircle(Brush.radialGradient(listOf(Lumen.accent.copy(alpha = 0.28f * a), Color.Transparent)), radius = size.minDimension / 2)
                },
        )
        Box(
            Modifier
                .align(Alignment.Center)
                .size(width = 18.dp, height = 30.dp)
                .graphicsLayer { alpha = 0.35f + 0.65f * a }
                .clip(RoundedCornerShape(4.dp))
                .background(Lumen.accent2),
        )
    }
}

@Composable
private fun SelectionBar(onCopy: () -> Unit, onPaste: () -> Unit) {
    Row(
        Modifier
            .clip(RoundedCornerShape(16.dp))
            .background(Lumen.surface3)
            .border(1.dp, Lumen.line2, RoundedCornerShape(16.dp))
            .padding(4.dp),
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        BarButton(Icons.copy, stringResource(R.string.copy), onCopy)
        Box(Modifier.width(1.dp).height(28.dp).align(Alignment.CenterVertically).background(Lumen.line2))
        BarButton(Icons.paste, stringResource(R.string.paste), onPaste)
    }
}

@Composable
private fun BarButton(icon: androidx.compose.ui.graphics.vector.ImageVector, text: String, onClick: () -> Unit) {
    Row(
        Modifier
            .clip(RoundedCornerShape(12.dp))
            .pressable(onClick, scaleTo = 0.92f)
            .padding(horizontal = 14.dp, vertical = 9.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, null, tint = Lumen.accent, modifier = Modifier.size(17.dp))
        Spacer(Modifier.width(8.dp))
        Text(text, style = MaterialTheme.typography.labelMedium, color = Lumen.fg)
    }
}

