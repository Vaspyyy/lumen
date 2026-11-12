---
title: Configuration
description: One TOML file. Everything in it is optional, and saving it is enough.
---

lumen reads one file:

```
~/.config/lumen/lumen.toml
```

If you have moved your config folder with `XDG_CONFIG_HOME`, the file is there. `lumen --config-path` prints the place.

The file doesn't have to exist. Without it, and for every setting you leave out, lumen uses its defaults.

## Saving is enough

lumen notices when the file is saved and takes the change on while it runs. It says so at the bottom of the window: `Config reloaded`.

Most settings change what is open: the font, the colors, the cursor, how see-through the window is. A few apply to what is opened next: `columns` and `lines` to the next window, `shell` to the next tab or pane.

## When there is a mistake in the file

A name lumen doesn't know, or a value of the wrong kind, is a mistake, and lumen tells you.

- **When lumen starts**, it starts with its defaults and prints what is wrong to the terminal it was started from. A terminal that won't start is how one gets locked out of fixing it.
- **When you save the file while lumen runs**, the settings from before stay, and the window says what is wrong.

## The whole file

Every setting, with the value lumen uses when you don't set it:

```toml
scrollback = 10000         # lines kept above the screen
copy_on_select = false     # selecting text also copies it
notifications = true       # programs may post on the desktop
# shell = "/usr/bin/fish"  # your own shell if unset
prewarm = false            # a new tab is there at once
launch = "tab"             # starting lumen again: tab, window, process

[font]
family = "monospace"       # the system's fixed-width font
size = 12.0                # in points
line_height = 1.0          # 1.2 gives the lines more air

[window]
opacity = 1.0              # of the background; 1 is solid
blur = true                # blur what is behind the window
padding = 10               # space around a pane's text, in pixels
columns = 100              # the size of a new window
lines = 30

[cursor]
style = "block"            # block, beam, underline
blink = true               # dims and comes back
glow = true                # the cursor gives off light
smooth = true              # the cursor glides between cells

[colors]
accent = "#ffc97a"         # lumen's own light
draw_bold_text_with_bright_colors = false

[colors.primary]
background = "#12111a"
foreground = "#e4e1ee"

[editor]
# command = "nvim"         # $VISUAL or $EDITOR if unset
# terminal = true          # by the editor's name if unset

[pulse]
enabled = true             # show the machine's load, where pulse is installed
every = 3                  # seconds between two looks
```

The `[colors]` table has more in it: the cursor's colors, the selection's, and the sixteen colors programs use. [Colors](/docs/colors/) has all of them.

## General

| Setting | Default | What it does |
| --- | --- | --- |
| `scrollback` | `10000` | How many lines are kept above the screen, per pane |
| `copy_on_select` | `false` | Selecting text puts it on the clipboard, too. It can always be pasted with a middle click |
| `notifications` | `true` | A program in a tab you are not looking at may post a notification on the desktop. With `false` the tab still gets its dot. See [Commands](/docs/commands/#when-a-program-has-something-to-tell) |
| `shell` | unset | The program a new tab or pane runs: its path, without arguments. Unset, it is your shell, as `$SHELL` names it |
| `prewarm` | `false` | Keeps one shell started ahead of time. See [Instant tabs](/docs/instant-tabs/) |
| `launch` | `"tab"` | What `lumen` opens when a lumen runs already: `"tab"`, a tab in the window you used last; `"window"`, a window of the lumen that runs; `"process"`, a lumen of its own. See [Opening lumen again](/docs/launching/) |

## `[font]`

| Setting | Default | What it does |
| --- | --- | --- |
| `family` | `"monospace"` | The font. `"monospace"` is the fixed-width font your desktop is set to |
| `size` | `12.0` | In points, from 4 to 96 |
| `line_height` | `1.0` | The height of a row, as a multiple of the font's own. From 0.8 to 3 |

More in [Fonts](/docs/fonts/).

## `[window]`

| Setting | Default | What it does |
| --- | --- | --- |
| `opacity` | `1.0` | How solid the background is, from 0 to 1. Text stays solid |
| `blur` | `true` | Blurs what is behind a see-through window, where the desktop can do that |
| `padding` | `10` | Space between a pane's edge and its text, in pixels. From 0 to 200 |
| `columns` | `100` | The width of a new window, counted in cells of the default 12 point font. With larger text, fewer fit. From 20 to 1000 |
| `lines` | `30` | The height of a new window, counted the same way. From 5 to 500 |

## `[cursor]`

| Setting | Default | What it does |
| --- | --- | --- |
| `style` | `"block"` | `"block"`, `"beam"` or `"underline"`. A program can ask for another shape while it runs |
| `blink` | `true` | The cursor dims and comes back. It never goes out, so it is never lost |
| `glow` | `true` | The cursor gives off light onto what is around it |
| `smooth` | `true` | The cursor glides to where it goes instead of jumping |

## `[editor]`

What a `Ctrl+Click` on a place in code, such as `src/main.rs:42`, opens. See [Files and links](/docs/files-and-links/#your-editor).

| Setting | Default | What it does |
| --- | --- | --- |
| `command` | unset | The editor: its name, with any arguments it should always get. For an editor lumen doesn't know, the whole command with `{file}`, `{line}` and `{column}` in it. Unset, it is what `$VISUAL` names, or else `$EDITOR` |
| `terminal` | unset | `true` if the editor runs in a terminal, and so gets a tab. `false` if it has a window of its own. Unset, lumen goes by the editor's name, and takes one it doesn't know for an editor of the terminal |

## `[pulse]`

What lumen shows of the machine's load, where pulse is installed. See [The machine](/docs/machine/).

| Setting | Default | What it does |
| --- | --- | --- |
| `enabled` | `true` | lumen starts pulse and shows what it says: the light in the title bar, and what each tab uses. With `false` it starts nothing |
| `every` | `3` | Seconds between two looks at the machine, from 1 to 60 |

A value outside its range is brought to the nearest one that is allowed.
