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

lumen has no terminfo entry of its own. It behaves as xterm does where programs look that up, so it works on machines that have never heard of it, over SSH as well. `TERM_PROGRAM` is not passed on over SSH unless you configure that. Another terminal that is started from a shell in lumen inherits it, unless it sets its own.

The better way to find out what a terminal can do is to ask it. lumen answers the queries the protocols below define for that.

## What lumen supports

| | |
| --- | --- |
| Colors | The 16 colors of the theme, the 256 numbered ones, and true color |
| Setting colors | A program can set colors anew for its pane: the 256 numbered ones (OSC 4), the text (OSC 10), the background (OSC 11) and the cursor (OSC 12). OSC 104, 110, 111 and 112 set them back. The background fills the whole pane |
| Text styles | Bold, italic, dim, inverse, hidden, strikethrough; straight and curly underlines, in a color of their own. A double underline is drawn as a thicker line, dotted and dashed ones as a straight one |
| Wide characters | Characters that take two cells, such as CJK and most emoji |
| Links | OSC 8. See [Files and links](/docs/files-and-links/) |
| Commands | The marks of OSC 133: where a prompt, a command's output and its end are. See [Marks for commands](#marks-for-commands) |
| Notifications | OSC 777 and OSC 9. See [Notifications](#notifications) |
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
| `A` | A prompt begins here |
| `B` | The prompt has ended, what is typed begins |
| `C` | The command was sent off, its output begins |
| `D` | The command has ended. `D;0` gives its exit status |
| `L` | What follows begins on a line of its own: if the cursor is not at the start of a line, lumen begins a new one |
| `P` | A prompt begins here, of the kind `k=` names. `P` and `P;k=i` say what `A` says. `P;k=r`, `P;k=c` and `P;k=s` begin a prompt beside what is typed: at the right of the line, or before a further line of the command |

Anything after a further `;` is left alone, with one exception: `C;cmdline_url=TEXT` gives the command's text, percent-encoded, as fish sends it. Without it, lumen reads the text from the screen, between `B` and `C`.

`B` can be left out. What was written before a `B` on its line is not part of what was typed.

- **`L` is for a shell that writes its prompt where the output stopped**, as bash does. Send it before the prompt and before anything else the shell writes on the prompt's line. `A` itself moves nothing. A command that runs when an `L` comes is over, and a `D` that follows before the next `A` or `C` says how it ended.
- **An `A` while a prompt is open** is either that prompt written again, as after a resize or under a list of completions, or a new prompt. Written again, it stays the same command.
  - After an `L` it is a new prompt.
  - If the open prompt came after an `L`, its shell is one that says when a prompt is new. An `A` without an `L` is then that prompt again wherever it comes, if the same fields follow the `A` as in the one that opened the prompt.
  - Any other `A` goes by where it stands: every `A` of a shell that sends no `L`, and one with other fields than the open prompt's, which is taken for another shell's, such as the one around a shell that has ended. It is the open prompt again on a line that is already that prompt's, anywhere else that is not below it, and on the first line of a screen that was just cleared. It is a new prompt below the lines of the open one, with at most two empty lines between.
- **What is left of a prompt that was written again** is the lines of its earlier copy right above it, with what stood typed there and what the shell printed under it. They are not a prompt of their own, nothing on them is read as typed, and they are no output of the command before. Where a shell writes only the last line of its prompt again, the lines above that one stay the prompt's.
- **A prompt of the kind `r`, `c` or `s`** is no part of the command. What is written from the `P` to the next `B` is not read as typed, and what is typed goes on after that `B`. Before a `c` or an `s`, what stands on its line is not read as typed either. Where no prompt is open, such a `P` says nothing. Send these only where `TERM_PROGRAM` is `lumen`: to a terminal that doesn't know them, the `B` that ends one is where the command begins.
- **A `D` where no command runs is ignored**, unless it is the one that follows an `L`. So a shell may say it with every prompt it writes.
- **An `A` or a `C` while a command runs**, with no `D` before it, is taken for a shell inside that command: the command is no longer followed.
- On the alternate screen the marks are ignored.

What lumen does with them is in [Commands](/docs/commands/). `lumen shell-integration bash` prints a small, complete example for a shell.

## Notifications

A program asks for the user's attention with one of:

```
ESC ] 777 ; notify ; TITLE ; TEXT ESC \
ESC ] 9 ; TEXT ESC \
```

Either may end in `BEL` instead. These are the sequences other terminals show as a desktop notification.

lumen shows one only when the tab is not being looked at, because another tab is open or the window is in the background: the tab gets a dot, and the desktop a notification that leads back to the pane. In the tab that is looked at, nothing happens, so say what you have to say in your output as well.

- The title is cut at 120 characters and the text at 400. Control characters are left out.
- The text is shown as it stands. `<`, `>` and `&` are not read as the desktop's markup.
- With OSC 9, or an OSC 777 whose title is empty, the title is the pane's own: the one a program set, or else the name of the folder the shell is in.
- An OSC 9 is ignored when its first field is empty or a number. The first field is the text up to the first semicolon, or all of it. Those are ConEmu's sequences, such as a progress bar. So a text that is only a number is not shown.
- One pane is heard on the desktop at most once in three seconds, and five times until its tab is looked at again.

## Links that put a command at the prompt

An OSC 8 link with the scheme `lumen-prompt` stands for a command:

```
lumen-prompt:COMMAND
```

`COMMAND` is the command's text, percent-encoded: `lumen-prompt:pulse%20tree%204821.kfx`. A click on the link types the text at the shell's prompt and does not run it.

- The text is one line of at most 300 characters. lumen shows all of it before the click, so nothing in it may be unseen: no control character, no space other than the plain one, and none of the characters without a shape of their own, such as those that turn the direction of writing, spaces without width and joiners. A link with anything else in it is not followed.
- It is a link where its text is. On an empty cell, on hidden text, and on text in the color of its background or one close to it, a click is not for the link. The colors are compared as they are drawn, however they were written. One space between two characters of it counts as text.
- It is a link only in the pane that has the keyboard, in the window in front. A click that brings a pane or a window to the front types nothing, nor does one in the first 300 ms after that, and a double click types once.
- Where the pane has no room to show the whole command above or below the link, it is not a link.
- Where lumen knows that a program runs in the pane, nothing is typed.
- Other terminals don't know the scheme and hand it to the desktop, which has nothing for it. Print these links only where `TERM_PROGRAM` is `lumen`.

## Places in code

lumen opens `path:line` in the user's editor on a `Ctrl+Click`, with no link around it. A tool that prints places as `path:line`, `path:line:column`, `path(line)` or `path(line,column)`, with the path relative to the folder it was run in, needs to do nothing more. A path with a `/` in it may also be relative to one of the five folders above. See [Files and links](/docs/files-and-links/#web-addresses-and-places-in-code).

## Not supported

- Sixel and iTerm2 pictures.
- Reading the clipboard with OSC 52.
- A bell you can hear. The bell puts a dot on a tab that is not being looked at, and does nothing else.
