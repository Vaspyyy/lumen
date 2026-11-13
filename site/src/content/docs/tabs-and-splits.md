---
title: Tabs and splits
description: Several terminals in one window, without a multiplexer.
---

A lumen window holds tabs, and a tab holds panes. Every pane is a terminal of its own, with its own shell, scrollback and text size.

## Tabs

| Key | Does |
| --- | --- |
| `Ctrl+Shift+T` | New tab, in the folder of the current pane (or the one `new_tab_folder` names) |
| `Ctrl+Tab`, `Ctrl+Shift+Tab` | Next and previous tab |
| `Ctrl+PageDown`, `Ctrl+PageUp` | Next and previous tab, too |
| `Alt+1` to `Alt+8` | Go to a tab by its place |
| `Alt+9` | Go to the last tab |
| `Ctrl+Shift+PageUp`, `Ctrl+Shift+PageDown` | Move the tab left or right |
| `Ctrl+Shift+Q` | Close the tab, with all its panes |

With the mouse: click a tab to go to it, click the `+` for a new one, and click the `×` on a tab or middle-click the tab to close it. Drag a tab onto another one to move it there.

A new tab opens to the right of the one you are in.

The tab bar can show only while there is more than one tab, or never: `bar` in the [`[tabs]`](/docs/configuration/#tabs) table.

A tab is named after the title its program sets. Most shells set one: the folder, or the command that is running. Where a program sets none, the tab shows the name of the folder the shell is in.

## Panes

| Key | Does |
| --- | --- |
| `Ctrl+Shift+D` | Split: a new pane to the right |
| `Ctrl+Shift+E` | Split: a new pane below |
| `Ctrl+Alt+Arrow` | Move to the pane in that direction |
| `Ctrl+Shift+Enter` | Let the current pane fill the tab, and back |
| `Ctrl+Shift+W` | Close the pane |

A split divides the current pane in two. Split again and that half is divided, so any layout can be built from the two keys.

The line between two panes can be dragged with the mouse to give one of them more room.

`Ctrl+Shift+Enter` is for the moment a pane is too small: it fills the tab, and the same key brings the layout back.

## Where a new one starts

A new tab or pane starts in the folder of the pane you were in. lumen asks the system where that pane's shell is, so this works with any shell and needs no setup.

To have new tabs start somewhere else, set [`new_tab_folder`](/docs/configuration/#general) to `"home"`, `"/"` or any folder, in the file or with `/settings`. A split still starts beside the pane it splits.

With [`prewarm`](/docs/instant-tabs/) on, the shell of the next tab or pane is started ahead of time, and is there the moment you ask for it.

## When a program ends

A pane closes when its program ends: when you leave the shell with `exit` or `Ctrl+D`. A tab closes with its last pane, and the window closes with its last tab.

lumen doesn't ask before it closes something. The window's close button closes every tab in it, and what runs in them ends.

## More than one window

`Ctrl+Shift+N` opens a new window. It starts in the folder its window was opened in, not in the folder of the current pane, with the current pane's environment.

Starting `lumen` again while it runs opens a tab in the window you used last, not a window: `lumen --new-window` opens one, and `launch = "window"` in the config makes that the rule. See [Opening lumen again](/docs/launching/).
