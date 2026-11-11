package io.github.vaspyyy.lumen.term

import io.github.vaspyyy.lumen.core.Core
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * The last snapshot of one terminal, read straight out of a direct buffer.
 * Each tab keeps its own, so a tab shows its last screen while it waits
 * for the connection to come back.
 */
class Screen {
    var buf: ByteBuffer = ByteBuffer.allocateDirect(64 * 1024).order(ByteOrder.LITTLE_ENDIAN)
        private set
    var valid = false
        private set

    var cols = 0; private set
    var rows = 0; private set
    var cursorCol = 0; private set
    var cursorRow = 0; private set
    var cursorShape = 0; private set
    var mode = 0; private set
    var displayOffset = 0; private set
    var historyLines = 0; private set
    var background = 0xFF12111A.toInt(); private set
    var foreground = 0xFFECE9F4.toInt(); private set
    var cursorColor = 0xFFFFC97A.toInt(); private set
    var cursorTextColor = 0xFF1C1305.toInt(); private set

    /** For each cell, 0 or the byte offset of its extra codepoints record + 1. */
    private var extras = IntArray(0)

    /** Takes a new snapshot. Allocates only when the screen outgrows the buffer. */
    fun take(tab: Long): Boolean {
        if (tab == 0L) return false
        var n = Core.snapshot(tab, buf)
        if (n < 0) {
            buf = ByteBuffer.allocateDirect(-n + 16 * 1024).order(ByteOrder.LITTLE_ENDIAN)
            n = Core.snapshot(tab, buf)
        }
        if (n < HEADER) return false
        parse(n)
        return true
    }

    private fun parse(len: Int) {
        val b = buf
        cols = b.u16(0)
        rows = b.u16(2)
        cursorCol = b.u16(4)
        cursorRow = b.u16(6)
        cursorShape = b.get(8).toInt() and 0xFF
        mode = b.u16(10)
        displayOffset = b.getInt(12)
        historyLines = b.getInt(16)
        background = b.getInt(20)
        foreground = b.getInt(24)
        cursorColor = b.getInt(28)
        cursorTextColor = b.getInt(32)
        val count = cols * rows
        if (extras.size < count) extras = IntArray(count) else java.util.Arrays.fill(extras, 0, count, 0)
        var off = HEADER + count * CELL
        var left = b.getInt(44)
        while (left-- > 0 && off + 8 <= len) {
            val cell = b.getInt(off)
            val n = b.getInt(off + 4)
            if (cell in 0 until count) extras[cell] = off + 1
            off += 8 + 4 * n
        }
        valid = cols > 0 && rows > 0 && HEADER + count * CELL <= len
    }

    fun selStartCol() = buf.u16(36)
    fun selStartRow() = buf.u16(38)

    fun cellOffset(col: Int, row: Int) = HEADER + (row * cols + col) * CELL
    fun codepoint(off: Int) = buf.getInt(off)
    fun fg(off: Int) = buf.getInt(off + 4)
    fun bg(off: Int) = buf.getInt(off + 8)
    fun flags(off: Int) = buf.getInt(off + 12)

    /** The extra codepoints of a cell: (offset of the first, count), packed; 0 if none. */
    fun extrasOf(col: Int, row: Int): Int = extras[row * cols + col]

    companion object {
        const val HEADER = 48
        const val CELL = 16
        const val NO_CURSOR = 0xFFFF

        const val MODE_ALT = 1
        const val MODE_MOUSE = 2
        const val MODE_SELECTION = 16

        const val BOLD = 1
        const val ITALIC = 2
        const val UNDERLINE = 4
        const val DOUBLE_UNDERLINE = 8
        const val CURLY_UNDERLINE = 16
        const val DOTTED_UNDERLINE = 32
        const val DASHED_UNDERLINE = 64
        const val STRIKE = 128
        const val WIDE = 256
        const val WIDE_SPACER = 512
        const val SELECTED = 1024
        const val LINK = 2048
        const val EXTRAS = 4096
        const val ANY_UNDERLINE = UNDERLINE or DOUBLE_UNDERLINE or CURLY_UNDERLINE or DOTTED_UNDERLINE or DASHED_UNDERLINE
    }
}

private fun ByteBuffer.u16(i: Int) = getShort(i).toInt() and 0xFFFF
