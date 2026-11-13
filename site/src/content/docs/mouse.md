---
title: Mouse
description: Selecting, pasting, scrolling, and what the mouse does on tabs and panes.
---

## Selecting text

| With the mouse | Selects |
| --- | --- |
| Drag | Text, character by character |
| Double click | A word. Keep the button down and drag to go on word by word |
| Triple click | A line. Drag to go on line by line |
| `Ctrl+Drag` | A block: a rectangle of text, for columns |

What you select can be pasted with a middle click right away, in lumen and in other programs. To put it on the clipboard, press `Ctrl+Shift+C`, or set `copy_on_select = true` in the [config](/docs/configuration/) and every selection is copied.

Where a double click stops is `word_separators` in the [`[selection]`](/docs/configuration/#selection) table.

## Pasting

| | |
| --- | --- |
| Middle click | Pastes what was selected last |
| `Ctrl+Shift+V`, `Shift+Insert` | Paste the clipboard |

Programs that ask for it are told where a paste starts and ends, so a shell or an editor does not run or indent pasted text line by line.

## Scrolling

The wheel scrolls through what the pane has printed. `scroll_multiplier` in the [`[mouse]`](/docs/configuration/#mouse) table makes it go faster or slower.

The pointer goes out of sight while you type, and comes back when the mouse moves. `hide_while_typing = false` in the same table keeps it in sight.

In a full-screen program that has no scrollback of its own to offer, such as `less` or `man`, the wheel moves through the program's text instead: lumen sends it the arrow keys.

## Programs that use the mouse

A program that asks for the mouse, such as vim, htop or a file manager, gets it: clicks, drags and the wheel go to the program.

Hold `Shift` and the mouse is lumen's again, to select text in such a program.

## Links and file names

| On a file name from a listing | Does |
| --- | --- |
| Click | Copies the file |
| `Ctrl+Click` | Opens it |
| Drag | Takes the file along |

Other links open with `Ctrl+Click`. So do web addresses, files and places in code such as `src/main.rs:42` that a program printed as plain text: hold `Ctrl`, and what lumen can follow is underlined under the mouse. See [Files and links](/docs/files-and-links/).

A link that stands for a command puts it at the prompt with a plain click, in the pane you are typing in, and leaves `Enter` to you.

## Commands

| On the mark beside a command | Does |
| --- | --- |
| Move the mouse onto it | Says how the command ended and how long it took |
| Click | Copies what the command printed |

See [Commands](/docs/commands/).

## Tabs and panes

| With the mouse | Does |
| --- | --- |
| Click a tab | Goes to it |
| Rest on a tab | Says what its programs use, where [pulse](/docs/machine/) is installed |
| Click the light in the title bar | Opens pulse in a tab |
| Middle click a tab | Closes it |
| Drag a tab onto another | Moves it there |
| Click a pane | Makes it the current one |
| Drag the line between two panes | Gives one of them more room |

## Dropping files

Files dropped onto a pane from a file manager are typed as their paths, quoted for the shell.
