package io.github.vaspyyy.lumen.model

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import io.github.vaspyyy.lumen.term.Screen
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

/**
 * One tab: a kept terminal on the PC (or a demo terminal). The handle
 * changes when the connection comes back; the tab, its title and its last
 * screen stay. Mutated on the main thread only, except [dirty].
 */
class Tab(val name: String, val demo: Boolean = false) {
    val id = ids.incrementAndGet()

    /** The core's handle, 0 while there is none (reconnecting). */
    var handle: Long = 0L

    /** Bumped on every new handle, so events from an old one are ignored. */
    var gen = 0

    var title by mutableStateOf("")
    /** Counts bells, for the pill to flash. */
    var bells by mutableIntStateOf(0)

    val screen = Screen()

    /** Set when the core says the screen changed; cleared when a snapshot is taken. */
    val dirty = AtomicBoolean(true)

    /** The view showing this tab, told (from any thread) that the screen changed. */
    @Volatile
    var onDirty: (() -> Unit)? = null

    var cols = 0
    var rows = 0

    val label: String get() = title.ifBlank { name }

    companion object {
        private val ids = AtomicInteger()
    }
}
