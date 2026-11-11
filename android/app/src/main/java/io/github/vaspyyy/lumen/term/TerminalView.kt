package io.github.vaspyyy.lumen.term

import android.annotation.SuppressLint
import android.content.Context
import android.content.res.Configuration
import android.graphics.Canvas
import android.graphics.DashPathEffect
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RadialGradient
import android.graphics.Shader
import android.graphics.Typeface
import android.graphics.fonts.Font
import android.graphics.fonts.FontFamily
import android.os.SystemClock
import android.text.InputType
import android.util.TypedValue
import android.view.GestureDetector
import android.view.HapticFeedbackConstants
import android.view.KeyCharacterMap
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import android.widget.OverScroller
import androidx.core.content.res.ResourcesCompat
import androidx.core.graphics.withScale
import androidx.core.graphics.withTranslation
import io.github.vaspyyy.lumen.R
import io.github.vaspyyy.lumen.core.Core
import io.github.vaspyyy.lumen.model.Tab
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sin

/**
 * Draws a terminal from its snapshot and turns touches and keys into what
 * the core understands. Hosted in Compose with AndroidView.
 *
 * Nothing in the draw path allocates: the snapshot is read in place, text
 * goes through reused char arrays, and paints, paths and shaders are made
 * when the font size or the cursor color changes.
 */
@SuppressLint("ViewConstructor")
class TerminalView(context: Context, private val mods: StickyMods) : View(context) {

    interface Host {
        /** The grid changed (settled: debounced while the keyboard animates). */
        fun onGrid(cols: Int, rows: Int, cellWidth: Int, cellHeight: Int)
        fun onScrolledBack(back: Boolean)
        /** A selection was made (show the bar near x,y in view pixels) or went away. */
        fun onSelection(shown: Boolean, x: Float, y: Float)
        /** Pinching: the font size so far; [done] when the fingers lift. */
        fun onFontSize(size: Float, done: Boolean)
        /**
         * A link was tapped, or (null) a tap went elsewhere. [target] is where
         * it leads: for a file on the PC (`file://`, or a name in a `glint`
         * listing) its path there, which can be copied; for anything else the
         * address, which can be opened. What a link shows and where it leads
         * are two things, so nothing is opened or copied on the tap itself.
         */
        fun onLink(target: String?, isFile: Boolean)
        fun copy(text: String)
        fun clipboard(): String?
    }

    var host: Host? = null

    private var tab: Tab? = null
    private val dm = resources.displayMetrics

    // ------------------------------------------------------------ fonts and metrics

    /**
     * The icons of prompts and listings (Nerd Fonts): characters no font of
     * the system has. They come after the terminal's font, before the system's.
     */
    private val symbols: FontFamily? = runCatching {
        FontFamily.Builder(Font.Builder(resources, R.font.symbols_nerd_font_mono).build()).build()
    }.getOrNull()

    private fun face(id: Int): Typeface {
        val plain = ResourcesCompat.getFont(context, id) ?: Typeface.MONOSPACE
        val icons = symbols ?: return plain
        return runCatching {
            Typeface.CustomFallbackBuilder(FontFamily.Builder(Font.Builder(resources, id).build()).build())
                .addCustomFallback(icons)
                .setSystemFallback("monospace")
                .build()
        }.getOrDefault(plain)
    }

    private val faces: Array<Typeface> = arrayOf(
        face(R.font.maple_mono_regular),
        face(R.font.maple_mono_bold),
        face(R.font.maple_mono_italic),
        face(R.font.maple_mono_bold_italic),
    )

    /** One paint per style: 0 regular, 1 bold, 2 italic, 3 bold italic. */
    private val textPaints = Array(4) { i ->
        Paint(Paint.ANTI_ALIAS_FLAG or Paint.SUBPIXEL_TEXT_FLAG or Paint.LINEAR_TEXT_FLAG).apply {
            typeface = faces[i]
            // Maple Mono draws its ligatures through these features; a terminal shows each character.
            fontFeatureSettings = "'liga' 0, 'calt' 0, 'clig' 0, 'dlig' 0"
            isLinearText = true
        }
    }
    private val bgPaint = Paint()
    private val linePaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val wavePaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    private val dotPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    private val dashPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    private val cursorPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val glowPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val wave = Path()

    var fontSize = 13f
        private set
    var cellW = 1
        private set
    var cellH = 1
        private set
    private var baseline = 0f
    private var underlineY = 0f
    private var lineThickness = 1f
    private var strikeY = 0f
    private var glowRadius = 1f
    private var glowColor = 0
    private var padX = (6 * dm.density).roundToInt()
    private var padTop = (8 * dm.density).roundToInt()
    private var originX = 0
    private var originY = 0

    /** 0 unknown, 1 drawn at exactly one cell by the font (can go in a text run), 2 not. */
    private val oneCell = ByteArray(0x10000)
    private val probe = CharArray(2)
    private var rowChars = CharArray(256)
    private val cellChars = CharArray(64)

    var cols = 0
        private set
    var rows = 0
        private set
    private var reportedCols = -1
    private var reportedRows = -1
    private var reportedCellW = -1
    private var reportedCellH = -1

    // ------------------------------------------------------------ state

    private var needSnapshot = true
    private var scrolledBack = false
    private var lastActivity = 0L
    private var selecting = false
    private var selCol = -1
    private var selRow = -1
    private var scaling = false
    private var scrollAccum = 0f
    private var flingLast = 0

    init {
        isFocusable = true
        isFocusableInTouchMode = true
        // With a hardware keyboard Android lays a gray veil over the focused
        // view: the terminal shows where the keys go with its cursor.
        defaultFocusHighlightEnabled = false
        isHapticFeedbackEnabled = true
        setFont(13f)
    }

    // ------------------------------------------------------------ binding

    private val dirtyListener: () -> Unit = { postInvalidateOnAnimation() }

    fun bind(t: Tab?) {
        if (t === tab) {
            // another view may have shown this tab meanwhile and taken its place
            if (t != null && isAttachedToWindow && t.onDirty !== dirtyListener) {
                t.onDirty = dirtyListener
                refresh()
            }
            return
        }
        unlisten()
        tab = t
        t?.onDirty = dirtyListener
        needSnapshot = true
        selecting = false
        host?.onSelection(false, 0f, 0f)
        invalidate()
    }

    /** Stops listening to the tab, unless another view listens to it by now. */
    private fun unlisten() {
        val t = tab ?: return
        if (t.onDirty === dirtyListener) t.onDirty = null
    }

    private val handle: Long get() = tab?.handle ?: 0L

    // ------------------------------------------------------------ font size and grid

    fun setFont(size: Float) {
        val sp = size.coerceIn(MIN_FONT, MAX_FONT)
        val px = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, sp, dm)
        val p = textPaints[0]
        p.textSize = px
        val adv = p.measureText("M")
        // Nudge the size so a cell is a whole number of pixels wide: runs of
        // text then land exactly on the grid, however long they are.
        val w = max(1, adv.roundToInt())
        val size2 = px * w / adv
        for (tp in textPaints) tp.textSize = size2
        val fm = p.fontMetrics
        val h = max(1, ceil(fm.descent - fm.ascent - 0.01f).toInt())
        fontSize = sp
        cellW = w
        cellH = h
        baseline = ((h - (fm.descent - fm.ascent)) / 2f - fm.ascent).roundToInt().toFloat()
        lineThickness = max(1f, (h / 16f).roundToInt().toFloat())
        val up = p.underlinePosition
        underlineY = baseline + if (up.isFinite() && up > 0) max(up, lineThickness) else max(1f, fm.descent * 0.4f)
        underlineY = min(underlineY, h - lineThickness)
        strikeY = baseline + fm.ascent * 0.28f
        wavePaint.strokeWidth = lineThickness
        dotPaint.strokeWidth = lineThickness
        dotPaint.pathEffect = DashPathEffect(floatArrayOf(lineThickness, lineThickness * 2), 0f)
        dashPaint.strokeWidth = lineThickness
        dashPaint.pathEffect = DashPathEffect(floatArrayOf(lineThickness * 4, lineThickness * 2), 0f)
        glowRadius = h * 1.9f
        glowColor = 0
        java.util.Arrays.fill(oneCell, 0)
        layoutGrid()
        invalidate()
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        layoutGrid()
    }

    /** The display's density or the system's font size changed: everything measured in pixels is measured again. */
    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        padX = (6 * dm.density).roundToInt()
        padTop = (8 * dm.density).roundToInt()
        setFont(fontSize)
    }

    private fun layoutGrid() {
        if (width == 0 || height == 0) return
        val c = max(1, (width - 2 * padX) / cellW)
        val r = max(1, (height - padTop) / cellH)
        originX = (width - c * cellW) / 2
        // what is left of the height goes half above the grid and half below
        originY = padTop + (height - padTop - r * cellH) / 2
        cols = c
        rows = r
        if (rowChars.size < c * 2) rowChars = CharArray(c * 2)
        if (c == reportedCols && r == reportedRows && cellW == reportedCellW && cellH == reportedCellH) return
        removeCallbacks(reportGrid)
        // The first size goes out at once; later ones wait for the keyboard
        // (or the pinch) to settle so the program is told the final size once.
        if (reportedCols < 0) reportGrid.run() else postDelayed(reportGrid, 140)
    }

    private val reportGrid = Runnable {
        reportedCols = cols
        reportedRows = rows
        reportedCellW = cellW
        reportedCellH = cellH
        host?.onGrid(cols, rows, cellW, cellH)
        needSnapshot = true
        invalidate()
    }

    // ------------------------------------------------------------ drawing

    override fun onDraw(canvas: Canvas) {
        val t = tab
        val s = t?.screen
        if (t != null && t.handle != 0L && (t.dirty.getAndSet(false) or needSnapshot)) {
            needSnapshot = false
            if (s!!.take(t.handle)) {
                lastActivity = SystemClock.uptimeMillis()
                if (rowChars.size < s.cols * 2) rowChars = CharArray(s.cols * 2)
            }
            val back = s.displayOffset > 0
            if (back != scrolledBack) {
                scrolledBack = back
                host?.onScrolledBack(back)
            }
        }
        if (s == null || !s.valid) {
            canvas.drawColor(SURFACE)
            return
        }
        canvas.drawColor(s.background)
        val rowsN = s.rows
        val colsN = s.cols
        for (r in 0 until rowsN) drawBackgrounds(canvas, s, r, colsN)

        val cursorVisible = s.cursorCol != Screen.NO_CURSOR && s.cursorRow < rowsN && s.cursorCol < colsN
        val blink = if (cursorVisible) blinkAlpha() else 1f
        if (cursorVisible) drawGlow(canvas, s, blink)

        for (r in 0 until rowsN) {
            drawText(canvas, s, r, colsN)
            drawDecorationRow(canvas, s, r, colsN)
        }

        if (cursorVisible) drawCursor(canvas, s, blink)
    }

    private fun drawBackgrounds(canvas: Canvas, s: Screen, r: Int, colsN: Int) {
        // A program that has the whole screen (an editor, a monitor) paints its
        // own background: the cells at the edges carry it out to the view's
        // edges, so no frame of the terminal's color is left around it.
        val whole = s.mode and Screen.MODE_ALT != 0
        val cellTop = (originY + r * cellH).toFloat()
        val top = if (whole && r == 0) 0f else cellTop
        val bottom = if (whole && r == s.rows - 1) height.toFloat() else cellTop + cellH
        val base = s.background
        var c = 0
        while (c < colsN) {
            val bg = s.bg(s.cellOffset(c, r))
            var e = c + 1
            while (e < colsN && s.bg(s.cellOffset(e, r)) == bg) e++
            if (bg != base) {
                bgPaint.color = bg
                val left = if (whole && c == 0) 0f else (originX + c * cellW).toFloat()
                val right = if (whole && e == colsN) width.toFloat() else (originX + e * cellW).toFloat()
                canvas.drawRect(left, top, right, bottom, bgPaint)
            }
            c = e
        }
    }

    private fun style(flags: Int) = (if (flags and Screen.BOLD != 0) 1 else 0) or (if (flags and Screen.ITALIC != 0) 2 else 0)

    /** Does the font draw this codepoint at exactly one cell? Then it can go in a text run. */
    private fun runnable(cp: Int, flags: Int): Boolean {
        if (flags and (Screen.WIDE or Screen.EXTRAS) != 0 || cp >= 0x10000 || cp < 0x20) return false
        var k = oneCell[cp].toInt()
        if (k == 0) {
            probe[0] = cp.toChar()
            val w = textPaints[0].measureText(probe, 0, 1)
            k = if (abs(w - cellW) < 0.02f * cellW + 0.05f) 1 else 2
            oneCell[cp] = k.toByte()
        }
        return k == 1
    }

    private fun drawText(canvas: Canvas, s: Screen, r: Int, colsN: Int) {
        val top = originY + r * cellH
        val y = top + baseline
        var c = 0
        while (c < colsN) {
            val off = s.cellOffset(c, r)
            val cp = s.codepoint(off)
            val flags = s.flags(off)
            if (flags and Screen.WIDE_SPACER != 0) { c++; continue }
            if (cp == 0 || cp == 32) { c++; continue }
            if (!runnable(cp, flags)) {
                drawCell(canvas, s, c, r, cp, s.fg(off), flags)
                c += if (flags and Screen.WIDE != 0) 2 else 1
                continue
            }
            // A run: this cell and the following ones with the same color and
            // style that the font draws at one cell each. Blanks inside a run
            // are kept as spaces, trailing ones are not.
            val fg = s.fg(off)
            val st = style(flags)
            var n = 0
            var last = 0
            var e = c
            while (e < colsN) {
                val o = s.cellOffset(e, r)
                val cp2 = s.codepoint(o)
                val f2 = s.flags(o)
                if (cp2 == 0 || cp2 == 32) {
                    if (f2 and (Screen.WIDE_SPACER or Screen.WIDE) != 0) break
                    rowChars[n++] = ' '
                    e++
                    continue
                }
                if (s.fg(o) != fg || style(f2) != st || !runnable(cp2, f2)) break
                rowChars[n++] = cp2.toChar()
                last = n
                e++
            }
            val p = textPaints[st]
            p.color = fg
            canvas.drawText(rowChars, 0, last, (originX + c * cellW).toFloat(), y, p)
            c += last.coerceAtLeast(1)
        }
    }

    private fun drawDecorationRow(canvas: Canvas, s: Screen, r: Int, colsN: Int) {
        val top = originY + r * cellH
        for (c in 0 until colsN) {
            val off = s.cellOffset(c, r)
            val flags = s.flags(off)
            if (flags and (Screen.ANY_UNDERLINE or Screen.STRIKE) == 0 || flags and Screen.WIDE_SPACER != 0) continue
            drawDecorations(canvas, s.fg(off), flags, originX + c * cellW, top, if (flags and Screen.WIDE != 0) 2 else 1)
        }
    }

    /** One cell drawn alone: wide characters, emoji, fallback glyphs, combining marks. Centered, shrunk if too wide. */
    private fun drawCell(canvas: Canvas, s: Screen, c: Int, r: Int, cp: Int, color: Int, flags: Int) {
        var n = Character.toChars(cp, cellChars, 0)
        val ex = s.extrasOf(c, r)
        if (ex != 0) {
            val off = ex - 1
            val count = s.buf.getInt(off + 4)
            for (i in 0 until count) {
                if (n + 2 > cellChars.size) break
                n += Character.toChars(s.buf.getInt(off + 8 + 4 * i), cellChars, n)
            }
        }
        val span = if (flags and Screen.WIDE != 0) 2 else 1
        val p = textPaints[style(flags)]
        p.color = color
        val x = (originX + c * cellW).toFloat()
        val top = (originY + r * cellH).toFloat()
        val target = (span * cellW).toFloat()
        val w = p.measureText(cellChars, 0, n)
        if (w <= target + 0.5f) {
            canvas.drawText(cellChars, 0, n, x + (target - w) / 2f, top + baseline, p)
        } else {
            val k = target / w
            canvas.withScale(k, k, x, top + cellH / 2f) {
                drawText(cellChars, 0, n, x, top + baseline, p)
            }
        }
    }

    private fun drawDecorations(canvas: Canvas, color: Int, flags: Int, x0: Int, top: Int, span: Int) {
        val x = x0.toFloat()
        val x2 = x + span * cellW
        val t = lineThickness
        val uy = top + underlineY
        linePaint.color = color
        when {
            flags and Screen.CURLY_UNDERLINE != 0 -> {
                wavePaint.color = color
                wave.reset()
                val amp = t * 1.1f
                val len = cellW.toFloat()
                var px = x
                val cy = uy - amp * 0.3f
                wave.moveTo(px, cy + amp * sin(px / len * TWO_PI))
                while (px < x2) {
                    px = min(px + 2f, x2)
                    wave.lineTo(px, cy + amp * sin(px / len * TWO_PI))
                }
                canvas.drawPath(wave, wavePaint)
            }
            flags and Screen.DOUBLE_UNDERLINE != 0 -> {
                canvas.drawRect(x, uy - t, x2, uy, linePaint)
                canvas.drawRect(x, uy + t, x2, uy + 2 * t, linePaint)
            }
            flags and Screen.DOTTED_UNDERLINE != 0 -> {
                dotPaint.color = color
                canvas.drawLine(x, uy, x2, uy, dotPaint)
            }
            flags and Screen.DASHED_UNDERLINE != 0 -> {
                dashPaint.color = color
                canvas.drawLine(x, uy, x2, uy, dashPaint)
            }
            flags and Screen.UNDERLINE != 0 -> canvas.drawRect(x, uy, x2, uy + t, linePaint)
        }
        if (flags and Screen.STRIKE != 0) {
            val sy = top + strikeY
            canvas.drawRect(x, sy - t / 2, x2, sy + t / 2, linePaint)
        }
    }

    private fun cursorSpan(s: Screen): Int {
        val f = s.flags(s.cellOffset(s.cursorCol, s.cursorRow))
        return if (f and Screen.WIDE != 0) 2 else 1
    }

    /** The lamp: a soft warm light around the cursor, under the text. */
    private fun drawGlow(canvas: Canvas, s: Screen, blink: Float) {
        val color = s.cursorColor or 0xFF000000.toInt()
        if (color != glowColor) {
            glowColor = color
            val rgb = color and 0x00FFFFFF
            glowPaint.shader = RadialGradient(
                0f, 0f, glowRadius,
                intArrayOf(0x66000000 or rgb, 0x2A000000 or rgb, 0x0C000000 or rgb, rgb),
                floatArrayOf(0f, 0.32f, 0.62f, 1f),
                Shader.TileMode.CLAMP,
            )
        }
        val focused = hasFocus()
        glowPaint.alpha = ((if (focused) 0.55f + 0.45f * blink else 0.35f) * 255).toInt()
        val cx = originX + (s.cursorCol + cursorSpan(s) / 2f) * cellW
        val cy = originY + (s.cursorRow + 0.5f) * cellH
        canvas.withTranslation(cx, cy) {
            scale(1.15f, 0.85f)
            drawCircle(0f, 0f, glowRadius, glowPaint)
        }
    }

    private fun drawCursor(canvas: Canvas, s: Screen, blink: Float) {
        val span = cursorSpan(s)
        val x = (originX + s.cursorCol * cellW).toFloat()
        val y = (originY + s.cursorRow * cellH).toFloat()
        val w = (span * cellW).toFloat()
        val focused = hasFocus()
        val alpha = if (focused) blink else 1f
        if (alpha <= 0.01f) return
        cursorPaint.color = s.cursorColor
        cursorPaint.alpha = (alpha * (s.cursorColor ushr 24)).toInt()
        val thick = max(2f, (1.6f * dm.density))
        val shape = if (!focused) 3 else s.cursorShape
        val round = min(cellW * 0.18f, 3f * dm.density)
        when (shape) {
            0 -> {
                cursorPaint.style = Paint.Style.FILL
                canvas.drawRoundRect(x, y, x + w, y + cellH, round, round, cursorPaint)
                val off = s.cellOffset(s.cursorCol, s.cursorRow)
                val cp = s.codepoint(off)
                if (cp != 0 && cp != 32) {
                    val flags = s.flags(off)
                    val ink = (s.cursorTextColor and 0x00FFFFFF) or ((alpha * 255).toInt() shl 24)
                    drawCell(canvas, s, s.cursorCol, s.cursorRow, cp, ink, flags and Screen.ANY_UNDERLINE.inv())
                }
            }
            1 -> {
                cursorPaint.style = Paint.Style.FILL
                canvas.drawRect(x, y + cellH - thick, x + w, y + cellH, cursorPaint)
            }
            2 -> {
                cursorPaint.style = Paint.Style.FILL
                canvas.drawRect(x, y, x + thick, y + cellH, cursorPaint)
            }
            else -> {
                cursorPaint.style = Paint.Style.STROKE
                cursorPaint.strokeWidth = max(1.5f, dm.density)
                val i = cursorPaint.strokeWidth / 2
                canvas.drawRoundRect(x + i, y + i, x + w - i, y + cellH - i, round, round, cursorPaint)
            }
        }
    }

    /**
     * The cursor stays lit while things happen and blinks, with a short fade,
     * once it has been quiet for a moment. Redraws only during fades. After
     * a while of nothing it stays lit and draws no more: a terminal left
     * open costs the battery nothing.
     */
    private fun blinkAlpha(): Float {
        if (!hasFocus()) return 1f
        val now = SystemClock.uptimeMillis()
        val quiet = now - lastActivity
        if (quiet < BLINK_AFTER) {
            postInvalidateDelayed(BLINK_AFTER - quiet + 1)
            return 1f
        }
        if (quiet > BLINK_UNTIL) return 1f
        val ph = ((quiet - BLINK_AFTER) % BLINK_PERIOD).toFloat() / BLINK_PERIOD
        // lit 0..0.5, fade out 0.5..0.6, dark 0.6..0.9, fade in 0.9..1
        return when {
            ph < 0.5f -> { postInvalidateDelayed(((0.5f - ph) * BLINK_PERIOD).toLong() + 1); 1f }
            ph < 0.6f -> { postInvalidateOnAnimation(); ease(1f - (ph - 0.5f) / 0.1f) }
            ph < 0.9f -> { postInvalidateDelayed(((0.9f - ph) * BLINK_PERIOD).toLong() + 1); 0f }
            else -> { postInvalidateOnAnimation(); ease((ph - 0.9f) / 0.1f) }
        }
    }

    private fun ease(t: Float) = t * t * (3 - 2 * t)

    private fun touched() {
        lastActivity = SystemClock.uptimeMillis()
    }

    private fun refresh() {
        needSnapshot = true
        postInvalidateOnAnimation()
    }

    // ------------------------------------------------------------ touch

    private fun colAt(x: Float) = ((x - originX) / cellW).toInt().coerceIn(0, max(0, cols - 1))
    private fun rowAt(y: Float) = ((y - originY) / cellH).toInt().coerceIn(0, max(0, rows - 1))

    private val scroller = OverScroller(context)

    private val gestures = GestureDetector(context, object : GestureDetector.SimpleOnGestureListener() {
        override fun onDown(e: MotionEvent): Boolean {
            if (!scroller.isFinished) scroller.forceFinished(true)
            scrollAccum = 0f
            return true
        }

        override fun onScroll(e1: MotionEvent?, e2: MotionEvent, dx: Float, dy: Float): Boolean {
            if (scaling || selecting) return true
            scrollBy(-dy, e2.x, e2.y)
            return true
        }

        override fun onFling(e1: MotionEvent?, e2: MotionEvent, vx: Float, vy: Float): Boolean {
            if (scaling || selecting) return true
            flingLast = 0
            flingX = e2.x
            flingY = e2.y
            scroller.fling(0, 0, 0, vy.toInt(), 0, 0, -1_000_000, 1_000_000)
            postInvalidateOnAnimation()
            return true
        }

        override fun onSingleTapUp(e: MotionEvent): Boolean {
            tap(e)
            return true
        }

        override fun onLongPress(e: MotionEvent) {
            if (scaling) return
            startSelection(e)
        }
    })

    private var flingX = 0f
    private var flingY = 0f

    override fun computeScroll() {
        if (scroller.computeScrollOffset()) {
            val y = scroller.currY
            scrollBy((y - flingLast).toFloat(), flingX, flingY)
            flingLast = y
            postInvalidateOnAnimation()
        }
    }

    /** Positive [dy]: the finger moved down, back into history. */
    private fun scrollBy(dy: Float, x: Float, y: Float) {
        val h = handle
        if (h == 0L) return
        scrollAccum += dy
        val lines = (scrollAccum / cellH).toInt()
        if (lines != 0) {
            scrollAccum -= lines * cellH
            Core.scroll(h, lines, colAt(x), rowAt(y))
            refresh()
        }
    }

    /**
     * Two fingers change the font size. Tracked by hand: the platform's scale
     * detector ignores pinches that start with the fingers closer than about
     * 27 mm, which is most of them on a phone.
     */
    private var pinchStartSpan = 0f
    private var pinchStartSize = 13f

    private fun span(e: MotionEvent): Float {
        val dx = e.getX(0) - e.getX(1)
        val dy = e.getY(0) - e.getY(1)
        return kotlin.math.sqrt(dx * dx + dy * dy)
    }

    private fun pinch(e: MotionEvent) {
        when (e.actionMasked) {
            MotionEvent.ACTION_POINTER_DOWN -> if (e.pointerCount == 2) {
                scaling = true
                pinchStartSpan = max(1f, span(e))
                pinchStartSize = fontSize
                scroller.forceFinished(true)
            }
            MotionEvent.ACTION_MOVE -> if (scaling && e.pointerCount >= 2) {
                val size = (pinchStartSize * span(e) / pinchStartSpan).coerceIn(MIN_FONT, MAX_FONT)
                val snapped = (size * 2).roundToInt() / 2f
                if (snapped != fontSize) {
                    setFont(snapped)
                    host?.onFontSize(snapped, false)
                }
            }
            MotionEvent.ACTION_POINTER_UP -> if (scaling && e.pointerCount == 2) {
                host?.onFontSize(fontSize, true)
            }
        }
    }

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(e: MotionEvent): Boolean {
        pinch(e)
        if (selecting) {
            when (e.actionMasked) {
                MotionEvent.ACTION_MOVE -> extendSelection(e.x, e.y)
                MotionEvent.ACTION_UP -> finishSelection(e.x, e.y)
                MotionEvent.ACTION_CANCEL -> selecting = false
            }
            gestures.onTouchEvent(e)
            return true
        }
        if (e.pointerCount > 1) {
            // Two fingers: a pinch, never a long press or a tap.
            val cancel = MotionEvent.obtain(e).apply { action = MotionEvent.ACTION_CANCEL }
            gestures.onTouchEvent(cancel)
            cancel.recycle()
            return true
        }
        if (!scaling) gestures.onTouchEvent(e)
        if (e.actionMasked == MotionEvent.ACTION_UP || e.actionMasked == MotionEvent.ACTION_CANCEL) {
            scaling = false
        }
        return true
    }

    private fun tap(e: MotionEvent) {
        val h = handle
        val s = tab?.screen
        if (h != 0L && s != null && s.valid && s.mode and Screen.MODE_SELECTION != 0) {
            Core.select(h, 2, 0, 0)
            host?.onSelection(false, 0f, 0f)
            refresh()
            return
        }
        host?.onSelection(false, 0f, 0f)
        showKeyboard()
        if (h == 0L) {
            host?.onLink(null, false)
            return
        }
        val col = colAt(e.x)
        val row = rowAt(e.y)
        if (Core.mouse(h, 0, 0, col, row, 0)) {
            Core.mouse(h, 1, 0, col, row, 0)
            host?.onLink(null, false)
            return
        }
        val link = Core.linkAt(h, col, row)
        val path = link?.let { pathOf(it) }
        host?.onLink(path ?: link, path != null)
    }

    /**
     * The file a link names, if it names one on the PC (`file://host/path`,
     * or what glint marks its names with): its path there is what is worth
     * copying on a phone.
     */
    private fun pathOf(link: String): String? {
        val rest = listOf("file://", "glint-copy://").firstNotNullOfOrNull { scheme ->
            link.takeIf { it.startsWith(scheme, true) }?.substring(scheme.length)
        } ?: return null
        val path = rest.substring(rest.indexOf('/').takeIf { it >= 0 } ?: return null)
        return runCatching { android.net.Uri.decode(path) }.getOrDefault(path).takeIf { it.isNotEmpty() }
    }

    private fun startSelection(e: MotionEvent) {
        val h = handle
        if (h == 0L) return
        selecting = true
        selCol = colAt(e.x)
        selRow = rowAt(e.y)
        performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
        host?.onSelection(false, 0f, 0f)
        Core.select(h, 0, selCol, selRow)
        refresh()
    }

    private fun extendSelection(x: Float, y: Float) {
        val h = handle
        if (h == 0L) return
        val c = colAt(x)
        val r = rowAt(y)
        if (c == selCol && r == selRow) return
        selCol = c
        selRow = r
        performHapticFeedback(HapticFeedbackConstants.TEXT_HANDLE_MOVE)
        Core.select(h, 1, c, r)
        refresh()
    }

    private fun finishSelection(x: Float, y: Float) {
        selecting = false
        val h = handle
        if (h == 0L) return
        if (Core.selection(h).isNullOrEmpty()) return
        // Show the bar above the start of the selection when it is in view.
        val s = tab?.screen
        val sr = s?.selStartRow() ?: Screen.NO_CURSOR
        val by = if (sr != Screen.NO_CURSOR) (originY + min(sr, rowAt(y)) * cellH).toFloat() else y
        host?.onSelection(true, x, by)
    }

    fun copySelection() {
        val h = handle
        if (h == 0L) return
        val text = Core.selection(h)
        if (!text.isNullOrEmpty()) host?.copy(text)
        Core.select(h, 2, 0, 0)
        host?.onSelection(false, 0f, 0f)
        refresh()
    }

    fun clearSelection() {
        val h = handle
        if (h != 0L) Core.select(h, 2, 0, 0)
        host?.onSelection(false, 0f, 0f)
        refresh()
    }

    fun paste() {
        val h = handle
        if (h == 0L) return
        val text = host?.clipboard()
        if (text.isNullOrEmpty()) return
        Core.paste(h, text)
        clearSelection()
        touched()
    }

    fun scrollToBottom() {
        val h = handle
        if (h == 0L) return
        scroller.forceFinished(true)
        Core.scrollToBottom(h)
        refresh()
    }

    // ------------------------------------------------------------ keyboard

    fun showKeyboard() {
        requestFocus()
        context.getSystemService(InputMethodManager::class.java)?.showSoftInput(this, 0)
    }

    override fun onFocusChanged(gainFocus: Boolean, direction: Int, previouslyFocusedRect: android.graphics.Rect?) {
        super.onFocusChanged(gainFocus, direction, previouslyFocusedRect)
        val h = handle
        if (h != 0L) Core.focus(h, gainFocus)
        invalidate()
    }

    override fun onCheckIsTextEditor() = true

    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection {
        outAttrs.inputType = InputType.TYPE_CLASS_TEXT or
            InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD or
            InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
        outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN or
            EditorInfo.IME_FLAG_NO_EXTRACT_UI or
            EditorInfo.IME_FLAG_NO_ENTER_ACTION or
            EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING or
            EditorInfo.IME_ACTION_NONE
        outAttrs.initialSelStart = 0
        outAttrs.initialSelEnd = 0
        return Input()
    }

    /**
     * What the soft keyboard sends. The terminal is not a text field: text
     * goes straight to the program, and keyboards that insist on composing
     * get the difference between the old and the new composing text.
     */
    private inner class Input : BaseInputConnection(this@TerminalView, false) {
        private var composing = ""

        override fun commitText(text: CharSequence, newCursorPosition: Int): Boolean {
            val t = text.toString()
            if (composing.isNotEmpty()) {
                replace(composing, t)
                composing = ""
            } else {
                typeText(t)
            }
            return true
        }

        override fun setComposingText(text: CharSequence, newCursorPosition: Int): Boolean {
            val t = text.toString()
            replace(composing, t)
            composing = t
            return true
        }

        override fun finishComposingText(): Boolean {
            composing = ""
            return true
        }

        override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
            if (composing.isNotEmpty()) {
                var end = (composing.length - beforeLength).coerceAtLeast(0)
                // never between the two halves of one character
                if (end in 1 until composing.length && Character.isLowSurrogate(composing[end])) end--
                keepComposing(end)
                return true
            }
            repeat(beforeLength.coerceAtMost(256)) { sendKey(KEY_BACKSPACE, mods.consume()) }
            repeat(afterLength.coerceAtMost(256)) { sendKey(KEY_DELETE, mods.consume()) }
            return true
        }

        override fun deleteSurroundingTextInCodePoints(beforeLength: Int, afterLength: Int): Boolean {
            if (composing.isNotEmpty()) {
                val had = composing.codePointCount(0, composing.length)
                keepComposing(composing.offsetByCodePoints(0, (had - beforeLength).coerceAtLeast(0)))
                return true
            }
            return deleteSurroundingText(beforeLength, afterLength)
        }

        private fun keepComposing(end: Int) {
            val keep = composing.substring(0, end)
            replace(composing, keep)
            composing = keep
        }

        override fun sendKeyEvent(event: KeyEvent): Boolean {
            if (event.action == KeyEvent.ACTION_DOWN) onKeyDown(event.keyCode, event)
            else if (event.action == KeyEvent.ACTION_MULTIPLE && event.keyCode == KeyEvent.KEYCODE_UNKNOWN) typeText(event.characters ?: "")
            return true
        }

        override fun getTextBeforeCursor(n: Int, flags: Int): CharSequence = ""
        override fun getTextAfterCursor(n: Int, flags: Int): CharSequence = ""

        /** Sends what turns [old] (already sent) into [new]: backspaces for what differs, then the rest. */
        private fun replace(old: String, new: String) {
            var p = 0
            val m = min(old.length, new.length)
            while (p < m && old[p] == new[p]) p++
            if (p > 0 && Character.isHighSurrogate(old[p - 1])) p--
            val drop = old.codePointCount(p, old.length)
            repeat(drop) { sendKey(KEY_BACKSPACE, 0) }
            if (p < new.length) typeText(new.substring(p))
        }
    }

    /** Typed text, with the extra row's Ctrl and Alt applied to it. Newlines are Enter. */
    private fun typeText(t: String) {
        val h = handle
        if (h == 0L || t.isEmpty()) return
        touched()
        var start = 0
        for (i in t.indices) {
            if (t[i] == '\n') {
                if (i > start) Core.text(h, t.substring(start, i), mods.consume())
                Core.key(h, KEY_ENTER, mods.consume())
                start = i + 1
            }
        }
        if (start < t.length) Core.text(h, t.substring(start), mods.consume())
        if (scrolledBack) refresh()
    }

    private fun sendKey(key: Int, m: Int) {
        val h = handle
        if (h == 0L) return
        touched()
        Core.key(h, key, m)
    }

    /** For the extra row. */
    fun pressKey(key: Int) {
        sendKey(key, mods.consume())
    }

    fun pressText(t: String) {
        typeText(t)
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        val h = handle
        if (h == 0L) return super.onKeyDown(keyCode, event)
        val meta = event.metaState
        val hw = (if (event.isShiftPressed) 1 else 0) or
            (if (meta and KeyEvent.META_ALT_MASK != 0) 2 else 0) or
            (if (meta and KeyEvent.META_CTRL_MASK != 0) 4 else 0)
        if (hw and 5 == 5) {
            when (keyCode) {
                KeyEvent.KEYCODE_C -> { copySelection(); return true }
                KeyEvent.KEYCODE_V -> { paste(); return true }
            }
        }
        val special = when (keyCode) {
            KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER -> KEY_ENTER
            KeyEvent.KEYCODE_TAB -> 2
            KeyEvent.KEYCODE_DEL -> KEY_BACKSPACE
            KeyEvent.KEYCODE_ESCAPE -> 4
            KeyEvent.KEYCODE_DPAD_UP -> 5
            KeyEvent.KEYCODE_DPAD_DOWN -> 6
            KeyEvent.KEYCODE_DPAD_RIGHT -> 7
            KeyEvent.KEYCODE_DPAD_LEFT -> 8
            KeyEvent.KEYCODE_MOVE_HOME -> 9
            KeyEvent.KEYCODE_MOVE_END -> 10
            KeyEvent.KEYCODE_PAGE_UP -> 11
            KeyEvent.KEYCODE_PAGE_DOWN -> 12
            KeyEvent.KEYCODE_INSERT -> 13
            KeyEvent.KEYCODE_FORWARD_DEL -> KEY_DELETE
            in KeyEvent.KEYCODE_F1..KeyEvent.KEYCODE_F12 -> 21 + (keyCode - KeyEvent.KEYCODE_F1)
            else -> 0
        }
        if (special != 0) {
            sendKey(special, hw or mods.consume())
            return true
        }
        val plain = event.getUnicodeChar(meta and (KeyEvent.META_CTRL_MASK or KeyEvent.META_ALT_MASK or KeyEvent.META_META_MASK).inv())
        // The right Alt is AltGr on many keyboards: with it, a German one types
        // @ { [ ] } \ | ~. When a key gives another character with it than
        // without, that character was typed, and Alt was no modifier.
        val altGr = meta and KeyEvent.META_ALT_RIGHT_ON != 0 && meta and KeyEvent.META_ALT_LEFT_ON == 0
        val graph = if (altGr) event.getUnicodeChar(meta and (KeyEvent.META_CTRL_MASK or KeyEvent.META_META_MASK).inv()) else 0
        val typed = graph != 0 && graph and KeyCharacterMap.COMBINING_ACCENT == 0 && graph != plain
        val uc = if (typed) graph else plain
        if (uc == 0 || uc and KeyCharacterMap.COMBINING_ACCENT != 0) return super.onKeyDown(keyCode, event)
        touched()
        val held = if (typed) hw and 2.inv() else hw
        Core.text(h, String(Character.toChars(uc)), (held and 1.inv()) or mods.consume())
        if (scrolledBack) refresh()
        return true
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
        if (handle != 0L && keyCode != KeyEvent.KEYCODE_BACK && event.keyCode !in VOLUME_KEYS) return true
        return super.onKeyUp(keyCode, event)
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        tab?.onDirty = dirtyListener
        refresh()
    }

    override fun onDetachedFromWindow() {
        super.onDetachedFromWindow()
        unlisten()
        removeCallbacks(reportGrid)
    }

    companion object {
        const val MIN_FONT = 7f
        const val MAX_FONT = 28f
        const val KEY_ENTER = 1
        const val KEY_BACKSPACE = 3
        const val KEY_DELETE = 14
        private const val BLINK_AFTER = 900L
        private const val BLINK_PERIOD = 1100L
        /** Blinking ends when this long has passed without anything typed or printed. */
        private const val BLINK_UNTIL = 20_000L
        private const val TWO_PI = (2 * Math.PI).toFloat()
        private const val SURFACE = 0xFF12111A.toInt()
        private val VOLUME_KEYS = intArrayOf(KeyEvent.KEYCODE_VOLUME_UP, KeyEvent.KEYCODE_VOLUME_DOWN, KeyEvent.KEYCODE_VOLUME_MUTE)
    }
}
