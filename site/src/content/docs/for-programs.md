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
| Text styles | Bold, italic, dim, inverse, hidden, strikethrough; straight and curly underlines, in a color of their own. A double underline is drawn as a thicker line, dotted and dashed ones as a straight one |
| Wide characters | Characters that take two cells, such as CJK and most emoji |
| Links | OSC 8. See [Files and links](/docs/files-and-links/) |
| Commands | The marks of OSC 133: where a prompt, a command's output and its end are. See [Marks for commands](#marks-for-commands) |
| Pictures | The kitty graphics protocol, in part. See [Pictures](/docs/images/) |
| Keyboard | The kitty keyboard protocol, all of its levels |
| Mouse | Click, drag and motion reports, in the SGR format and the older one |
| Clipboard | A program can set the clipboard (OSC 52). It can't read it |
| Window title | OSC 0 and 2. The title is the tab's name |
| Paste | Bracketed paste |
| Focus | Reports when the pane gains or loses the keyboard, to programs that ask |
| Screen updates | Synchronized updates, so a redraw is shown whole |
| Screens | The alternate screen, with the wheel as arrow keys in it |
| Cursor | The shapes a program can ask for: block, beam, underline. Whether it blinks is set in the config, not by the program |
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
- The text of the link has to contain the file's name. A link that reads `click here` is not copied by a plain click. A name that was cut short is fine when it ends in `…` and at least three characters of it are left.

In other terminals the link is handed to the desktop, where [glint](https://github.com/Vaspyyy/glint) can register itself to do the copying (`glint --init copy`).

## Marks for commands

A shell tells lumen where its commands are with the marks of OSC 133, each as `ESC ] 133 ; X ESC \` or ending in `BEL`:

| Mark | Says |
| --- | --- |
| `A` | A prompt begins here. If the cursor is not at the start of a line, lumen begins a new line first |
| `B` | The prompt has ended, what is typed begins |
| `C` | The command was sent off, its output begins |
| `D` | The command has ended. `D;0` gives its exit status |

Anything after a further `;` is left alone, with one exception: `C;cmdline_url=TEXT` gives the command's text, percent-encoded, as fish sends it. Without it, lumen reads the text from the screen, between `B` and `C`.

`B` can be left out. A prompt that is written again in the same place, as after a resize, stays the same command. An `A` or a `C` that comes while a command runs, with no `D` before it, is taken for a shell inside that command: the command is no longer followed. On the alternate screen the marks are ignored.

What lumen does with them is in [Commands](/docs/commands/). `lumen shell-integration bash` prints a small, complete example for a shell.

## Places in code

lumen opens `path:line` in the user's editor on a `Ctrl+Click`, with no link around it. A tool that prints places as `path:line`, `path:line:column`, `path(line)` or `path(line,column)`, with the path relative to the folder it was run in, needs to do nothing more. A path with a `/` in it may also be relative to one of the five folders above. See [Files and links](/docs/files-and-links/#web-addresses-and-places-in-code).

## Not supported

- Sixel and iTerm2 pictures.
- Reading the clipboard with OSC 52.
- A bell you can hear. The bell puts a dot on a tab that is not being looked at, and does nothing else.
