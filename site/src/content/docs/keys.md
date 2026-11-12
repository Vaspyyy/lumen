---
title: Keys
description: Every key lumen takes for itself. Everything else goes to the program in the pane.
---

lumen keeps few keys for itself. Where a program is likely to want a key, lumen's has `Shift` in it as well: `Ctrl+C` stops a program, `Ctrl+Shift+C` copies. A few plain ones, such as `Alt+1` to `Alt+9` and `Ctrl+Tab`, are keys that some programs use too. The keys can't be changed yet.

## Tabs

| Key | Does |
| --- | --- |
| `Ctrl+Shift+T` | New tab, in the folder of the current pane |
| `Ctrl+Tab` | Next tab |
| `Ctrl+Shift+Tab` | Previous tab |
| `Ctrl+PageDown`, `Ctrl+PageUp` | Next and previous tab, too |
| `Alt+1` to `Alt+8` | Go to a tab by its place |
| `Alt+9` | Go to the last tab |
| `Ctrl+Shift+PageUp` | Move the tab to the left |
| `Ctrl+Shift+PageDown` | Move the tab to the right |
| `Ctrl+Shift+Q` | Close the tab |

## Panes

| Key | Does |
| --- | --- |
| `Ctrl+Shift+D` | Split: a new pane to the right |
| `Ctrl+Shift+E` | Split: a new pane below |
| `Ctrl+Alt+Left`, `Ctrl+Alt+Right`, `Ctrl+Alt+Up`, `Ctrl+Alt+Down` | Move to the pane in that direction |
| `Ctrl+Shift+Enter` | Let the current pane fill the tab, and back |
| `Ctrl+Shift+W` | Close the pane. The tab closes with its last pane |

## Text

| Key | Does |
| --- | --- |
| `Ctrl+Shift+C` | Copy the selection |
| `Ctrl+Shift+V` | Paste |
| `Shift+Insert` | Paste, too |
| `Ctrl+Shift+F` | Search what the pane has printed. See [Search](/docs/search/) |

## Scrolling

| Key | Does |
| --- | --- |
| `Shift+PageUp` | Scroll up a page |
| `Shift+PageDown` | Scroll down a page |
| `Shift+Home` | Scroll to the top of the scrollback |
| `Shift+End` | Scroll to the bottom |

Typing brings the view back to the bottom.

## Commands

| Key | Does |
| --- | --- |
| `Ctrl+Shift+Up` | Go to the command before |
| `Ctrl+Shift+Down` | Go to the next command |
| `Ctrl+Shift+O` | Copy what the last command printed |

These are lumen's where the shell marks its commands. In a full-screen program, and in a shell that marks nothing, they go to the program. See [Commands](/docs/commands/).

## Text size

| Key | Does |
| --- | --- |
| `Ctrl+Plus` | Larger text in the current pane, a tenth at a time |
| `Ctrl+Minus` | Smaller text in the current pane |
| `Ctrl+0` | Back to the size from the config |

The size is per pane, lasts until the pane closes, and goes from less than half to four times the size in the config. The size all panes start with is `size` in the [`[font]`](/docs/fonts/) table.

## Windows

| Key | Does |
| --- | --- |
| `Ctrl+Shift+N` | New window, in the folder lumen was started in |

## Keys for the program

Every other key goes to the program in the pane. Programs that ask for it get keys in the [kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/), which can tell apart keys that look the same to an old terminal: `Tab` and `Ctrl+I`, `Enter` and `Shift+Enter`, `Esc` and the start of an escape sequence. Editors such as Neovim and Helix use it when it is there. See [For programs](/docs/for-programs/).
