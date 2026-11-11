# lumen-mobile

lumen's terminal and its connection to a PC, as a library for the Android
app in `android/`. The app draws and listens to the fingers; everything
behind that is here, in Rust:

| File | What it is |
| --- | --- |
| `src/screen.rs` | One terminal's screen: `alacritty_terminal` takes in what the program prints; keys, touches and pastes become the bytes a program expects; `snapshot` hands the app what to draw. |
| `src/link.rs` | The connection: SSH (the `russh` crate) to the PC's own sshd. Pairing, logging in with a signature the phone's secure hardware makes, the terminals the PC keeps. |
| `src/jni.rs` | What the app calls: `io.github.vaspyyy.lumen.core.Core`. |
| `src/palette.rs` | lumen's colors, the same as the desktop's defaults. |
| `examples/phone.rs` | A phone on the command line, for trying the PC side without Android. |

Build it for Android with `android/build-core.sh`. On a PC it builds and
tests like any crate: `cargo test -p lumen-mobile`.

## What a snapshot looks like

`Core.snapshot(tab, buffer)` writes what is to be seen into a direct
`ByteBuffer`, little endian, and returns the bytes written, or minus the
bytes needed if the buffer is too small.

The header, 48 bytes:

| At | Type | What |
| --- | --- | --- |
| 0 | u16 | columns |
| 2 | u16 | rows |
| 4 | u16 | the cursor's column, `0xFFFF` if there is no cursor to draw |
| 6 | u16 | the cursor's row |
| 8 | u8 | the cursor's shape: 0 block, 1 underline, 2 beam, 3 hollow block |
| 9 | u8 | unused |
| 10 | u16 | mode bits: 1 a program has the screen (alternate screen), 2 the program wants the mouse, 4 application cursor keys, 8 bracketed paste, 16 there is a selection, 32 the program wants focus events |
| 12 | u32 | lines scrolled back (0: at the bottom) |
| 16 | u32 | lines of history above the screen |
| 20 | u32 | background, `0xAARRGGBB` |
| 24 | u32 | foreground |
| 28 | u32 | the cursor's color |
| 32 | u32 | the color of the character under a block cursor |
| 36 | u16 ×4 | the selection's first cell (column, row) and last cell (column, row), `0xFFFF` where that is out of view or there is none |
| 44 | u32 | how many cells hold more than one character |

Then `rows × columns` cells of 16 bytes, row by row:

| At | Type | What |
| --- | --- | --- |
| 0 | u32 | the character, 0 for an empty cell |
| 4 | u32 | foreground, `0xAARRGGBB` |
| 8 | u32 | background |
| 12 | u32 | flags |

The colors are final: inverse, dim, hidden and the selection are applied.
The flags: 1 bold, 2 italic, 4 underline, 8 double underline, 16 curly
underline, 32 dotted underline, 64 dashed underline, 128 strikeout, 256
wide (takes two columns), 512 the second half of a wide character (draw
nothing), 1024 selected, 2048 part of a link, 4096 holds more characters.

Then, for each cell that holds more than one character (an accent on a
letter, an emoji's variation selector): `u32` the cell's index
(`row × columns + column`), `u32` how many follow, and that many `u32`
characters to draw after the cell's own.
