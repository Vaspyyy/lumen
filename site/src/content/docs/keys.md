---
title: Keys
description: Every key lumen takes for itself, and how to change them. Everything else goes to the program in the pane.
---

lumen keeps few keys for itself. Where a program is likely to want a key, lumen's has `Shift` in it as well: `Ctrl+C` stops a program, `Ctrl+Shift+C` copies. A few plain ones, such as `Alt+1` to `Alt+9` and `Ctrl+Tab`, are keys that some programs use too. Every key can be changed: see [Changing the keys](#changing-the-keys).

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

These are lumen's in a pane whose shell has marked a prompt or a command. In a full-screen program, and in a pane where nothing was ever marked, they go to the program. See [Commands](/docs/commands/).

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
| `Ctrl+Shift+N` | New window, in the folder the current window was opened in |

## Changing the keys

The `[keys]` table in the [config](/docs/configuration/) gives a key to one of lumen's actions, to text, or back to the program:

```toml
[keys]
"alt-t" = "new_tab"                    # an action, by its name below
"ctrl-shift-t" = "none"                # lumen leaves the key to the program
"shift-enter" = { send = "\u001b\r" }  # text for the program, as if typed
```

A key named in `[keys]` no longer does what lumen gave it, and the other keys stay as they are. Saving the file is enough.

Several keys one after the other are one binding too: `"ctrl-shift-k c" = "copy_output"`. lumen waits after the first key for the rest, so the first is no longer the program's: take one that programs don't use (with `Shift`, as lumen's own keys have). A key of lumen's that begins such a binding, such as `ctrl-shift-t` for `"ctrl-shift-t c"`, gives up its action, or it would wait too.

A key is written as GPUI writes it: the modifiers `ctrl`, `alt`, `shift` and `super`, each followed by a dash, then the key. Letters, digits and signs are themselves (`ctrl-shift-t`, `alt-1`, `ctrl-=`, `ctrl--` for the minus key). The other keys have names: `enter`, `tab`, `space`, `backspace`, `delete`, `escape`, `insert`, `home`, `end`, `pageup`, `pagedown`, `up`, `down`, `left`, `right`, `menu`, `print`, `pause`, `back`, `forward`, and `f1` to `f35`. A key lumen doesn't know, or an action it doesn't have, is a mistake, and lumen says so.

`send` takes text as TOML writes it: `\u001b` is `Esc`, `\r` is `Enter`, `\t` is `Tab`.

The actions, with lumen's keys for them:

| Action | Does | Key |
| --- | --- | --- |
| `copy` | Copy the selection | `Ctrl+Shift+C` |
| `paste` | Paste | `Ctrl+Shift+V`, `Shift+Insert` |
| `search` | Search what the pane has printed | `Ctrl+Shift+F` |
| `scroll_page_up`, `scroll_page_down` | Scroll a page | `Shift+PageUp`, `Shift+PageDown` |
| `scroll_line_up`, `scroll_line_down` | Scroll a line | |
| `scroll_top`, `scroll_bottom` | Scroll to the top or the bottom | `Shift+Home`, `Shift+End` |
| `zoom_in`, `zoom_out`, `zoom_reset` | Text size of the pane | `Ctrl+Plus`, `Ctrl+Minus`, `Ctrl+0` |
| `previous_command`, `next_command` | Go to the command before or after | `Ctrl+Shift+Up`, `Ctrl+Shift+Down` |
| `copy_output` | Copy what the last command printed | `Ctrl+Shift+O` |
| `new_tab` | New tab | `Ctrl+Shift+T` |
| `close_tab` | Close the tab | `Ctrl+Shift+Q` |
| `next_tab`, `previous_tab` | Next and previous tab | `Ctrl+Tab`, `Ctrl+Shift+Tab`, `Ctrl+PageDown`, `Ctrl+PageUp` |
| `move_tab_left`, `move_tab_right` | Move the tab | `Ctrl+Shift+PageUp`, `Ctrl+Shift+PageDown` |
| `tab_1` to `tab_8` | Go to a tab by its place | `Alt+1` to `Alt+8` |
| `last_tab` | Go to the last tab | `Alt+9` |
| `split_right`, `split_down` | Split the pane | `Ctrl+Shift+D`, `Ctrl+Shift+E` |
| `close_pane` | Close the pane | `Ctrl+Shift+W` |
| `focus_left`, `focus_right`, `focus_up`, `focus_down` | Move to the pane in that direction | `Ctrl+Alt+Arrow` |
| `zoom_pane` | Let the pane fill the tab, and back | `Ctrl+Shift+Enter` |
| `new_window` | New window | `Ctrl+Shift+N` |
| `open_pulse` | Open [pulse](/docs/machine/) in a tab, as a click on the light in the title bar does | |
| `quit` | Close every window | |

The keys for commands (`previous_command`, `next_command`, `copy_output`) stay lumen's only where a shell marks its commands, outside full-screen programs, whatever key they are given. The ones for a pane work where a pane has the keyboard, the rest anywhere in the window.

## Keys for the program

Every other key goes to the program in the pane. Programs that ask for it get keys in the [kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/), which can tell apart keys that look the same to an old terminal: `Tab` and `Ctrl+I`, `Enter` and `Shift+Enter`, `Esc` and the start of an escape sequence. Editors such as Neovim and Helix use it when it is there. See [For programs](/docs/for-programs/).
