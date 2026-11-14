---
title: Configuration
description: One TOML file. Everything in it is optional, and saving it is enough.
---

lumen reads one file:

```
~/.config/lumen/lumen.toml
```

If you have moved your config folder with `XDG_CONFIG_HOME`, the file is there. `lumen --config-path` prints the place.

## The settings menu

`lumen settings`, or `/settings` typed at a prompt, opens the most used settings in a menu: the arrows move and change, Enter flips a switch or lets you type (a font's name, a folder), `r` puts one back to its default, and `q` leaves. Each change is written to the file at once, so the window behind the menu shows it as you go. What else the file says stays as it is: your comments, the order, and the settings the menu doesn't show. A setting the file doesn't set is shown dim, at its default. The window's frame, the tab bar, a picture behind the text and the cursor's light are in the menu too; the keys, `[env]` and the colors are only in the file. [Looks](/docs/looks/) has all of how lumen looks.

`/settings` is taken only as those exact words at a prompt, with Enter: the shell never sees them, and a path like `/usr/bin/htop` still goes to the shell. It needs the shell to mark its prompts (fish 4.0 and newer do; bash and zsh need [one line](/docs/commands/#setting-up-your-shell)). Typed at a prompt, `/settings` opens the menu in a tab of its own, which closes when you leave it.

The file doesn't have to exist. Without it, and for every setting you leave out, lumen uses its defaults.

## Saving is enough

lumen notices when the file is saved and takes the change on while it runs. It says so at the bottom of the window: `Config reloaded`.

Most settings change what is open: the font, the colors, the cursor, the keys, the frame and how see-through the window is. A few apply to what is opened next: `columns`, `lines` and `startup_mode` to the next window, `shell` and `[env]` to the next tab or pane.

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
clipboard = "copy"         # what programs may do with it: copy, paste, both, off
# shell = "/usr/bin/fish"  # your own shell if unset
prewarm = false            # a new tab is there at once
launch = "tab"             # starting lumen again: tab, window, process
new_tab_folder = "current" # where a new tab starts: current, home, or a folder

[font]
family = "monospace"       # the system's fixed-width font
size = 12.0                # in points
line_height = 1.0          # 1.2 gives the lines more air
ligatures = true           # letters a font joins, such as ->, are joined
weight = 400               # of plain text: 100 (thin) to 900 (black)
bold_weight = 700          # of bold text
letter_spacing = 0         # pixels between two characters, beyond the font's own
# ui_family = "Inter"      # lumen's own words; the desktop's font if unset
ui_size = 12.5             # their size, in pixels

[window]
opacity = 1.0              # of the background; 1 is solid
blur = true                # blur what is behind the window
padding = 10               # space around a pane's text, in pixels; or { x = 10, y = 4 }
dynamic_padding = false    # what the last whole cell leaves goes to both sides
decorations = "lumen"      # who draws the frame: lumen, desktop, none
startup_mode = "windowed"  # windowed, maximized, fullscreen
columns = 100              # the size of a new window
lines = 30
corner_radius = 12         # how round lumen's frame is: 0 is square
border = true              # the thin line around it
shadow = true              # the shadow around it
# background_image = "~/Pictures/wall.png"   # a picture behind the text
background_image_opacity = 0.3   # how much of it shows: 1 is all
background_image_fit = "cover"   # cover, contain, fill
# gradient = "#3a1030"     # the background fades into this color
gradient_angle = 180       # which way: 180 from the top down

[tabs]
bar = "always"             # always; auto, with two tabs or more; never
position = "top"           # top, bottom

[panes]
dim_inactive = 0.22        # how far other panes step back: 0 not at all
divider_width = 1          # the line between panes, in pixels

[cursor]
style = "block"            # block, beam, underline
blink = true               # dims and comes back
blink_interval = 530       # milliseconds lit, and as many dimmed
glow = true                # the cursor gives off light
smooth = true              # the cursor glides between cells
rounded = true             # round corners
# thickness = 2            # of a beam or an underline, in pixels
unfocused = "hollow"       # in a pane without the keyboard: hollow, same, hidden
glow_strength = 1.0        # how bright the glow is: 0 to 3
glow_size = 1.0            # how far it reaches
glide_time = 70            # milliseconds a glide takes
blink_dim = 0.22           # how bright a blink is at its dimmest: 0 goes out
blink_fade = 140           # milliseconds to dim and to come back

[mouse]
scroll_multiplier = 1.0    # how far a turn of the wheel scrolls
hide_while_typing = true   # the pointer hides while you type

[selection]
word_separators = ",│`|:\"' ()[]{}<>\t"   # where a double click stops

[colors]
accent = "#ffc97a"         # lumen's own light
draw_bold_text_with_bright_colors = false

[colors.primary]
background = "#12111a"
foreground = "#e4e1ee"

[colors.ui]
# text = "#ffffff"         # lumen's own parts: see Looks

[editor]
# command = "nvim"         # $VISUAL or $EDITOR if unset
# terminal = true          # by the editor's name if unset

[pulse]
enabled = true             # show the machine's load, where pulse is installed
every = 3                  # seconds between two looks

[env]
# TERM = "xterm-256color"  # variables every new terminal gets

[keys]
# "ctrl-shift-t" = "none"  # see Keys
```

The `[colors]` table has more in it: the cursor's colors, the selection's, and the sixteen colors programs use. [Colors](/docs/colors/) has all of them, and [Looks](/docs/looks/) the colors of lumen's own parts, the picture, the gradient and the cursor's light, with a few looks to start from.

## General

| Setting | Default | What it does |
| --- | --- | --- |
| `scrollback` | `10000` | How many lines are kept above the screen, per pane |
| `copy_on_select` | `false` | Selecting text puts it on the clipboard, too. It can always be pasted with a middle click |
| `notifications` | `true` | A program in a tab you are not looking at may post a notification on the desktop. With `false` the tab still gets its dot. See [Commands](/docs/commands/#when-a-program-has-something-to-tell) |
| `clipboard` | `"copy"` | What a program may do with the clipboard (OSC 52). `"copy"`: put text on it, so that a program on a server you are logged in to can copy to your desktop. `"paste"`: read it. `"both"`, or `"off"`. A program that reads the clipboard sees anything you copied, passwords too |
| `shell` | unset | The program a new tab or pane runs: its path, without arguments. Unset, it is your shell, as `$SHELL` names it |
| `prewarm` | `false` | Keeps one shell started ahead of time. See [Instant tabs](/docs/instant-tabs/) |
| `launch` | `"tab"` | What `lumen` opens when a lumen runs already: `"tab"`, a tab in the window you used last; `"window"`, a window of the lumen that runs; `"process"`, a lumen of its own. See [Opening lumen again](/docs/launching/) |
| `new_tab_folder` | `"current"` | Where a new tab starts: `"current"`, the folder of the pane you are in; `"home"`, your home folder; or a folder, such as `"/"` or `"~/Projekte"`. A split always starts beside the pane it splits. A folder that isn't there (on a drive that isn't plugged in) is left aside for that tab. See [Tabs and splits](/docs/tabs-and-splits/#where-a-new-one-starts) |

## `[font]`

| Setting | Default | What it does |
| --- | --- | --- |
| `family` | `"monospace"` | The font. `"monospace"` is the fixed-width font your desktop is set to |
| `size` | `12.0` | In points, from 4 to 96 |
| `line_height` | `1.0` | The height of a row, as a multiple of the font's own. From 0.8 to 3 |
| `ligatures` | `true` | Letters a font joins into one sign, such as `->` into an arrow, are joined. With `false` each stays as it is |
| `weight` | `400` | How heavy plain text is, from 100 (thin) to 900 (black). 400 is regular. A font draws the weight nearest to this that it has |
| `bold_weight` | `700` | How heavy bold text is |
| `letter_spacing` | `0` | Pixels between two characters, beyond what the font has, from -5 to 20. Each character stays at the left of its cell, which is wider by this |
| `ui_family` | unset | The font of lumen's own words: the tabs, the notes, the search. Unset, or one that can't be found, it is your desktop's |
| `ui_size` | `12.5` | Their size, in pixels, from 8 to 18 |

More in [Fonts](/docs/fonts/).

## `[window]`

| Setting | Default | What it does |
| --- | --- | --- |
| `opacity` | `1.0` | How solid the background is, from 0 to 1. Text stays solid |
| `blur` | `true` | Blurs what is behind a see-through window, where the desktop can do that |
| `padding` | `10` | Space between a pane's edge and its text, in pixels, from 0 to 200. One number is the same on every side. `{ x = 10, y = 4 }` sets left and right (`x`) apart from top and bottom (`y`), as in Alacritty. The [marks of commands](/docs/commands/) stand in the space at the left, and need 4 pixels of it |
| `dynamic_padding` | `false` | A pane is seldom a whole number of cells wide and high. What is left over is at the right and the bottom; with `true` it is split between both sides, so the text sits in the middle |
| `decorations` | `"lumen"` | Who draws the window's frame. `"lumen"`: round corners, a shadow, and the window's buttons in the tab bar. `"desktop"`: the desktop's own title bar and frame, where it has them (KDE Plasma and most tiling desktops; on GNOME lumen draws its own). `"none"`: no frame, no shadow, no buttons and no edges to drag, and the window fills all the space the desktop gives it (on X11 without a compositor, the window manager draws its frame whatever lumen asks). On a tiling desktop that leaves a gap around lumen, `"none"` takes it away |
| `startup_mode` | `"windowed"` | How a new window opens: `"windowed"`, `"maximized"` or `"fullscreen"` |
| `columns` | `100` | The width of a new window, counted in cells of the default 12 point font. With larger text, fewer fit. From 20 to 1000 |
| `lines` | `30` | The height of a new window, counted the same way. From 5 to 500 |
| `corner_radius` | `12` | How round the corners of lumen's frame are, in pixels, from 0 (square) to 40 |
| `border` | `true` | A thin line around lumen's frame |
| `shadow` | `true` | lumen's frame casts a shadow around the window. It is drawn around the window only, never under it, so a see-through window is as see-through as `opacity` says |
| `background_image` | unset | A picture behind the text: its file, with `~` for your home folder; a path not starting at `/` or `~` starts in the folder of this file. PNG, JPEG, WebP, SVG and the other usual kinds. See [Looks](/docs/looks/#a-picture-behind-the-text) |
| `background_image_opacity` | `0.3` | How much of the picture shows over the background's color, from 0 to 1. On a see-through window, the two together are as see-through as `opacity` says |
| `background_image_fit` | `"cover"` | `"cover"`: all of the window, cut to fit. `"contain"`: all of the picture, with room beside it. `"fill"`: stretched to the window |
| `gradient` | unset | A color the background fades into, from its own (`background` in [`[colors.primary]`](/docs/colors/)) |
| `gradient_angle` | `180` | Which way it fades, in degrees: `180` from the top down, `90` from the left, `0` from the bottom up |

## `[tabs]`

| Setting | Default | What it does |
| --- | --- | --- |
| `bar` | `"always"` | When the tab bar is there. `"auto"`: while a window has two tabs or more. `"never"`: not at all; the keys in [Keys](/docs/keys/) still open, close and switch tabs. Where lumen draws the frame, the tab bar is also where the window's buttons are and where it is dragged: without it, your desktop's keys move and close the window. The [machine's light](/docs/machine/) is in it too; `open_pulse` in [`[keys]`](/docs/keys/#changing-the-keys) opens pulse without it |
| `position` | `"top"` | Where the tab bar is: `"top"` or `"bottom"` of the window |

## `[panes]`

| Setting | Default | What it does |
| --- | --- | --- |
| `dim_inactive` | `0.22` | How far the panes you aren't typing in step back, from 0 (not at all) to 1 |
| `divider_width` | `1` | The line between two panes, in pixels, from 0 (none) to 8. It can be dragged however thin it is |

## `[cursor]`

| Setting | Default | What it does |
| --- | --- | --- |
| `style` | `"block"` | `"block"`, `"beam"` or `"underline"`. A program can ask for another shape while it runs |
| `blink` | `true` | The cursor dims and comes back. It never goes out, so it is never lost |
| `blink_interval` | `530` | How long the cursor is lit, and then dimmed, in milliseconds. From 100 to 5000 |
| `glow` | `true` | The cursor gives off light onto what is around it |
| `smooth` | `true` | The cursor glides to where it goes instead of jumping |
| `rounded` | `true` | The cursor's corners are round. With `false` they are square |
| `thickness` | unset | How thick a beam or an underline cursor is, in pixels, from 1 to 20. Unset, a ninth of a row |
| `unfocused` | `"hollow"` | The cursor of a pane that doesn't have the keyboard. `"hollow"`: the outline of a block. `"same"`: its own shape, still, without the glow. `"hidden"`: none |
| `glow_strength` | `1.0` | How bright the cursor's light is, from 0 (none) to 3 |
| `glow_size` | `1.0` | How far the light reaches, from 0.2 to 4 |
| `glide_time` | `70` | Milliseconds the cursor takes to glide to its next cell, from 10 to 1000, where `smooth` is on |
| `blink_dim` | `0.22` | How bright a blinking cursor is at its dimmest, from 0 (it goes out, as in other terminals) to 1 |
| `blink_fade` | `140` | Milliseconds it takes to dim and to come back, from 0 (at once) to 1000 |

## `[mouse]`

| Setting | Default | What it does |
| --- | --- | --- |
| `scroll_multiplier` | `1.0` | How far a turn of the wheel scrolls, as a multiple of what your desktop says. From 0.1 to 20 |
| `hide_while_typing` | `true` | The mouse pointer goes out of sight while you type, and comes back when the mouse moves |

## `[selection]`

| Setting | Default | What it does |
| --- | --- | --- |
| `word_separators` | ``",│`\|:\"' ()[]{}<>\t"`` | The characters a double click stops at. A word is a run of any others, so a path is one word; with `/` among them, a double click takes one part of it |

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

## `[env]`

Variables every new tab and pane gets in its environment, as text (the terminals a [phone](/docs/kept-terminals/) opens don't):

```toml
[env]
TERM = "xterm"
EDITOR = "hx"
```

They are set after lumen's own, so they can change those too: `TERM` for a machine you log in to that doesn't know `xterm-256color`, for example.

## `[keys]`

Gives keys to lumen's actions, to text for the program, or back to the program. See [Changing the keys](/docs/keys/#changing-the-keys).

```toml
[keys]
"ctrl-shift-t" = "none"                # goes to the program now
"alt-t" = "new_tab"
"shift-enter" = { send = "\u001b\r" }  # Esc and Enter, for a new line in some programs
```

A value outside its range is brought to the nearest one that is allowed.
