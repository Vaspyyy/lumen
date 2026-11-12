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

## Pasting

| | |
| --- | --- |
| Middle click | Pastes what was selected last |
| `Ctrl+Shift+V`, `Shift+Insert` | Paste the clipboard |

Programs that ask for it are told where a paste starts and ends, so a shell or an editor does not run or indent pasted text line by line.

## Scrolling

The wheel scrolls through what the pane has printed.

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
| Middle click a tab | Closes it |
| Drag a tab onto another | Moves it there |
| Click a pane | Makes it the current one |
| Drag the line between two panes | Gives one of them more room |

## Dropping files

Files dropped onto a pane from a file manager are typed as their paths, quoted for the shell.
