package io.github.vaspyyy.lumen.core

import java.nio.ByteBuffer

/** Signs with the phone's key. Called on a background thread; may block (for the fingerprint prompt). */
interface Signer {
    /** The DER-encoded ECDSA signature (what `Signature.getInstance("SHA256withECDSA")` returns) over `data`, or null to give up. */
    fun sign(data: ByteArray): ByteArray?
}

/** What a terminal tells the app. Every method is called on a background thread, never the main thread. */
interface TabEvents {
    fun dirty()                      // the screen changed: redraw (called once until the next snapshot is taken)
    fun title(title: String)         // the program set a title ("" = none)
    fun bell()
    fun clipboard(text: String)      // the program asks to put this on the clipboard (OSC 52)
    fun ended(status: Int)           // the terminal's program ended; the tab is over
    fun lost(reason: String)         // the connection went away under it; the terminal on the PC lives on
}

object Core {
    init { System.loadLibrary("lumen_mobile") }

    external fun version(): String

    /** The `authorized_keys` form ("ecdsa-sha2-nistp256 AAAA...") of a P-256 public key given as `PublicKey.getEncoded()`. "" if it is not one. */
    external fun publicKey(spki: ByteArray): String

    /** The fingerprint of a public key line, as `ssh-keygen -l` and `lumen pair` on the PC show it: "SHA256:...". "" if it is not a key. */
    external fun fingerprint(publicKey: String): String

    /** Reads a pairing code without using it. {"ok":true,"label":"desk","user":"ada","port":22,"addresses":["192.168.1.20","100.64.0.7"]} or {"ok":false,"error":"..."} */
    external fun offer(link: String): String

    /** BLOCKING, up to about 4 minutes (the PC's user is asked to confirm there). Introduces this phone's key to the PC.
     *  {"ok":true,"host":{...}} : store the "host" object verbatim, it is what `connect` takes.
     *  {"ok":false,"code":"unreachable|wrong-host|refused|denied|cancelled|failed","error":"words for the user"} */
    external fun enroll(link: String, device: String, publicKey: String): String
    /** How far the `enroll` under way has come: 0 none, 1 reaching the PC, 2 the PC has the key and its user is being asked. */
    external fun enrollStage(): Int
    /** Ends the `enroll` under way: it returns at once with the code "cancelled", and the PC stops asking. */
    external fun cancelEnroll()

    /** BLOCKING. Connects and logs in. `host` is the stored host object as a JSON string. `signer.sign` is called once.
     *  {"ok":true,"link":7,"keeps":true,"address":"192.168.1.20"} : "keeps" says whether the PC keeps terminals running (it has lumen); if false, tabs are plain shells that end with the connection. "address" is the one that answered.
     *  {"ok":false,"code":"unreachable|wrong-host|refused|cancelled|failed","error":"words for the user"} ("refused": the PC no longer accepts this phone, it was unpaired there) */
    external fun connect(host: String, signer: Signer): String
    external fun disconnect(link: Long)
    /** BLOCKING up to timeoutMs: does the PC still answer? */
    external fun alive(link: Long, timeoutMs: Int): Boolean
    /** BLOCKING. The terminals the PC keeps: [{"name":"s24-1","title":"","cwd":"/home/ada/x","attached":0,"created":1794400000}] ("[]" if none or if the PC keeps none) */
    external fun sessions(link: Long): String
    /** BLOCKING. Ends a kept terminal on the PC. */
    external fun kill(link: Long, name: String)

    /** Opens a terminal on the link: attaches to the kept terminal `name`, starting it if there is none. Returns at once with a tab handle; the screen fills in as data arrives. `name`: letters, digits, '-', '_', '.', at most 64. */
    external fun open(link: Long, name: String, cols: Int, rows: Int, cellWidth: Int, cellHeight: Int, events: TabEvents): Long
    /** A terminal with no PC behind it: it shows a sample screen and echoes what is typed. For building and showing the UI. */
    external fun demo(cols: Int, rows: Int, events: TabEvents): Long
    /** Lets go of a tab (the terminal on the PC stays). The handle is dead afterwards. */
    external fun close(tab: Long)
    external fun resize(tab: Long, cols: Int, rows: Int, cellWidth: Int, cellHeight: Int)   // cell size in pixels

    /** Typed text. mods: 1 shift, 2 alt, 4 ctrl (ctrl+letter becomes the control character, alt prefixes ESC). */
    external fun text(tab: Long, text: String, mods: Int)
    /** Pasted text (bracketed if the program asked for that). */
    external fun paste(tab: Long, text: String)
    /** A key that is not text. key: 1 Enter, 2 Tab, 3 Backspace, 4 Escape, 5 Up, 6 Down, 7 Right, 8 Left, 9 Home, 10 End, 11 PageUp, 12 PageDown, 13 Insert, 14 Delete, 21..32 F1..F12. mods as above. */
    external fun key(tab: Long, key: Int, mods: Int)
    /** Scrolls by `lines` (positive: back into history). Where a program has the screen (vim, less, htop) it gets wheel or arrow keys instead; col,row is the cell under the finger. */
    external fun scroll(tab: Long, lines: Int, col: Int, row: Int)
    external fun scrollToBottom(tab: Long)
    /** A touch as a mouse event, if the program asked for the mouse. kind: 0 press, 1 release, 2 move. button: 0 left. Returns false if the program does not want the mouse (then the touch is yours). */
    external fun mouse(tab: Long, kind: Int, button: Int, col: Int, row: Int, mods: Int): Boolean
    /** Selection. kind: 0 start by word at the cell, 1 extend to the cell, 2 clear, 3 start by character, 4 start by line. */
    external fun select(tab: Long, kind: Int, col: Int, row: Int)
    external fun selection(tab: Long): String?
    /** The link under a cell (an OSC 8 hyperlink or a URL in the text), or null. */
    external fun linkAt(tab: Long, col: Int, row: Int): String?
    external fun focus(tab: Long, focused: Boolean)

    /** Writes the visible screen into a DIRECT ByteBuffer (little endian). Returns the bytes written, or -(bytes needed) if it is too small. */
    external fun snapshot(tab: Long, into: ByteBuffer): Int
}
