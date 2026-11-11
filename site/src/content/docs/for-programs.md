---
title: For programs
description: What a program running in lumen can rely on, and how it can tell it is in lumen.
---

This page is for people who write command line tools, and for anyone who wants to know why a program behaves as it does in lumen.

## How to tell

| Variable | Value |
| --- | --- |
| `TERM` | `xterm-256color` |
| `COLORTERM` | `truecolor` |
| `TERM_PROGRAM` | `lumen` |
| `TERM_PROGRAM_VERSION` | lumen's version, such as `0.2.1` |

lumen has no terminfo entry of its own. It behaves as xterm does where programs look that up, so it works on machines that have never heard of it, over SSH as well. `TERM_PROGRAM` is not passed on over SSH unless you configure that.

The better way to find out what a terminal can do is to ask it. lumen answers the queries the protocols below define for that.

## What lumen supports

| | |
| --- | --- |
| Colors | The 16 colors of the theme, the 256 numbered ones, and true color |
| Text styles | Bold, italic, dim, inverse, hidden, strikethrough; single, double and curly underlines, in a color of their own |
| Wide characters | Characters that take two cells, such as CJK and most emoji |
| Links | OSC 8. See [Files and links](/docs/files-and-links/) |
| Pictures | The kitty graphics protocol, in part. See [Pictures](/docs/images/) |
| Keyboard | The kitty keyboard protocol, all of its levels |
| Mouse | Click, drag and motion reports, in the SGR format and the older one |
| Clipboard | A program can set the clipboard (OSC 52). It can't read it |
| Window title | OSC 0 and 2. The title is the tab's name |
| Paste | Bracketed paste |
| Focus | Reports when the pane gains or loses the keyboard, to programs that ask |
| Screen updates | Synchronized updates, so a redraw is shown whole |
| Screens | The alternate screen, with the wheel as arrow keys in it |
| Cursor | The shapes a program can ask for: block, beam, underline, blinking or not |
| Queries | The colors in use, the size of the window in cells and in pixels, what kind of terminal this is |

## The kitty keyboard protocol

A program that turns it on gets keys in a form that leaves nothing unclear: `Tab` and `Ctrl+I` are different, as are `Enter` and `Shift+Enter`, and `Esc` is not the start of something else. lumen supports all five of the protocol's levels: telling keys apart, reporting when a key is repeated or let go, alternate keys, every key as an escape code, and the text a key types.

As in kitty, `Enter`, `Tab` and `Backspace` without a modifier stay as they always were unless the program asks for every key as an escape code. That way a shell is still usable when a program crashes and leaves the protocol switched on.

One thing differs from kitty: a symbol typed with `Shift`, such as `!`, is reported as that symbol, not as the key it is on with `Shift` held.

## Links that copy

A program can mark a file name so that a click on it in lumen copies the file. The link is an ordinary OSC 8 link with the scheme `glint-copy`:

```
glint-copy://HOSTNAME/absolute/path/to/file
```

Three rules, which keep such links from being misused:

- The host has to be the machine lumen runs on, or `localhost`, or empty.
- The path is absolute, and percent-encoded as in any URL.
- The text of the link has to contain the file's name. A link that reads `click here` is not copied by a plain click.

In other terminals the link is handed to the desktop, where [glint](https://github.com/Vaspyyy/glint) can register itself to do the copying (`glint --init copy`).

## Not supported

- Sixel and iTerm2 pictures.
- Reading the clipboard with OSC 52.
- The bell. It is silent, and nothing flashes.
- Marks for where a command's output starts and ends (OSC 133). They are what the next part of the [roadmap](/docs/roadmap/) builds on.
