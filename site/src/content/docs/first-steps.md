---
title: First steps
description: Ten minutes with lumen. The keys that matter, and the three things worth trying first.
---

## Open it

Start lumen from your application menu, or from another terminal:

```sh
lumen                       # a window with your shell
lumen -d ~/Projects/site    # start in a folder
lumen -e htop               # run a program instead of the shell
```

Started again while it runs, lumen opens a new tab in the window you used last instead of a second lumen. See [Opening lumen again](/docs/launching/).

The window has a frame of its own: the tabs are in the title bar, next to the buttons that minimize, maximize and close. A tab is named after what its program calls itself, or after the folder the shell is in.

## Tabs and panes

A tab holds one or more panes. Each pane is a terminal of its own.

| Key | Does |
| --- | --- |
| `Ctrl+Shift+T` | New tab |
| `Ctrl+Shift+D` | Split: a new pane to the right |
| `Ctrl+Shift+E` | Split: a new pane below |
| `Ctrl+Alt+Arrow` | Move to the pane in that direction |
| `Ctrl+Tab` | Next tab |
| `Ctrl+Shift+W` | Close the pane |

A new tab or pane starts in the folder of the pane you were in. [Tabs and splits](/docs/tabs-and-splits/) has the rest.

## Copy, paste, scroll

Drag with the mouse to select. `Ctrl+Shift+C` copies, `Ctrl+Shift+V` pastes. `Shift+PageUp` and `Shift+PageDown` scroll through what has been printed, and so does the mouse wheel.

A program that uses the mouse itself, such as vim or htop, gets the mouse. Hold `Shift` to select text anyway.

## Three things to try

**Click a file name.** This needs [glint](https://github.com/Vaspyyy/glint), and one line in its config (`~/.config/glint/config.toml`):

```toml
hyperlink = "copy"
```

Then list a folder with `glint` and click a name. The file is on your clipboard: paste it into a folder in your file manager, into a mail, into a chat. `Ctrl+Click` opens it instead. More in [Files and links](/docs/files-and-links/).

**Look at pictures.** In a folder with images:

```sh
glint --thumbs
```

The thumbnails are drawn in the terminal and scroll with the text. More in [Pictures](/docs/images/).

**Search.** Press `Ctrl+Shift+F` and type. lumen finds the text in everything the pane has printed, also what has scrolled out of view. More in [Search](/docs/search/).

## Make it yours

lumen works without a config file. To change something, create `~/.config/lumen/lumen.toml`:

```toml
prewarm = true          # new tabs are there at once

[font]
family = "JetBrains Mono"
size = 13.0

[window]
opacity = 0.9
```

Save the file, and lumen takes the change on while it runs. [Configuration](/docs/configuration/) lists every setting.
