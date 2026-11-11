package io.github.vaspyyy.lumen.term

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/** Ctrl and Alt on the extra row: a tap arms one for the next key, a second tap locks it. */
class StickyMods {
    enum class State { Off, Armed, Locked }

    var ctrl by mutableStateOf(State.Off)
    var alt by mutableStateOf(State.Off)

    fun tap(isCtrl: Boolean) {
        val s = if (isCtrl) ctrl else alt
        val next = when (s) {
            State.Off -> State.Armed
            State.Armed -> State.Locked
            State.Locked -> State.Off
        }
        if (isCtrl) ctrl = next else alt = next
    }

    /** The modifiers for the key being sent now (2 alt, 4 ctrl); armed ones are used up. */
    fun consume(): Int {
        var m = 0
        if (ctrl != State.Off) m = m or 4
        if (alt != State.Off) m = m or 2
        if (ctrl == State.Armed) ctrl = State.Off
        if (alt == State.Armed) alt = State.Off
        return m
    }
}
