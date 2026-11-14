---
title: Colors
description: lumen's own colors, how to change them, and how to use a theme written for Alacritty.
---

lumen comes with one theme of its own: a night sky with a warm lamp in it. The background is a very dark violet, the text a soft white, and the cursor is the lamp.

Everything about it can be changed in the `[colors]` table of the [config file](/docs/configuration/).

## A theme from Alacritty

The `[colors]` table has the same shape as [Alacritty's](https://alacritty.org/config-alacritty.html), on purpose. There are hundreds of themes written for Alacritty, for instance in the [alacritty-theme](https://github.com/alacritty/alacritty-theme) collection, and each of them works in lumen.

Open the theme's `.toml` file, copy its `[colors...]` tables, and paste them into `lumen.toml`. That is all:

```toml
[colors.primary]
background = "#1e1e2e"
foreground = "#cdd6f4"

[colors.cursor]
text = "#1e1e2e"
cursor = "#f5e0dc"

[colors.normal]
black = "#45475a"
red = "#f38ba8"
green = "#a6e3a1"
yellow = "#f9e2af"
blue = "#89b4fa"
magenta = "#f5c2e7"
cyan = "#94e2d5"
white = "#bac2de"
```

Save, and lumen changes its colors while you look at it.

A theme can hold tables for parts of Alacritty that lumen doesn't have (`search`, `hints`, `vi_mode_cursor`, `footer_bar`, `line_indicator`, `dim`). lumen accepts them and leaves them aside, so nothing has to be deleted from what you paste. This needs lumen 0.2.1 or newer.

`indexed_colors` is taken as Alacritty takes it: each entry sets one of the colors from 16 to 255, which are otherwise the usual cube and gray ramp.

```toml
[[colors.indexed_colors]]
index = 16
color = "#fab387"
```

lumen has no `import` line as Alacritty has. The tables go into `lumen.toml` itself.

## How a color is written

As `"#rrggbb"` or `"0xrrggbb"`, in quotes: `"#ffc97a"`.

## What can be set

### The background and the text

```toml
[colors.primary]
background = "#12111a"
foreground = "#e4e1ee"
```

### The cursor

```toml
[colors.cursor]
cursor = "#ffd9a0"      # the cursor itself
text = "#12111a"        # the character under a block cursor
```

### The selection

```toml
[colors.selection]
background = "#3a3550"
text = "#ffffff"
```

Unset, as it is by default, selected text keeps its colors and gets a light wash of the text color behind it.

### The sixteen colors

Programs color their output with eight colors and a brighter version of each. `ls`, `git` and your prompt use these.

```toml
[colors.normal]
black = "#232130"
red = "#f27a93"
green = "#8fdfa8"
yellow = "#ffd08a"
blue = "#8cb4ff"
magenta = "#c9a4ff"
cyan = "#7fd9e6"
white = "#d5d1e3"

[colors.bright]
black = "#6c6880"
red = "#ff9db0"
green = "#b0f0c2"
yellow = "#ffe0b0"
blue = "#b0ccff"
magenta = "#dcc4ff"
cyan = "#a8e9f2"
white = "#f6f4fc"
```

Those are lumen's defaults:

| | Normal | Bright |
| --- | --- | --- |
| Black | <i class="sw" style="background:#232130"></i> `#232130` | <i class="sw" style="background:#6c6880"></i> `#6c6880` |
| Red | <i class="sw" style="background:#f27a93"></i> `#f27a93` | <i class="sw" style="background:#ff9db0"></i> `#ff9db0` |
| Green | <i class="sw" style="background:#8fdfa8"></i> `#8fdfa8` | <i class="sw" style="background:#b0f0c2"></i> `#b0f0c2` |
| Yellow | <i class="sw" style="background:#ffd08a"></i> `#ffd08a` | <i class="sw" style="background:#ffe0b0"></i> `#ffe0b0` |
| Blue | <i class="sw" style="background:#8cb4ff"></i> `#8cb4ff` | <i class="sw" style="background:#b0ccff"></i> `#b0ccff` |
| Magenta | <i class="sw" style="background:#c9a4ff"></i> `#c9a4ff` | <i class="sw" style="background:#dcc4ff"></i> `#dcc4ff` |
| Cyan | <i class="sw" style="background:#7fd9e6"></i> `#7fd9e6` | <i class="sw" style="background:#a8e9f2"></i> `#a8e9f2` |
| White | <i class="sw" style="background:#d5d1e3"></i> `#d5d1e3` | <i class="sw" style="background:#f6f4fc"></i> `#f6f4fc` |

A program can also use any of 256 numbered colors, or any color at all. Those need no setting.

### The accent

```toml
[colors]
accent = "#ffc97a"
```

The accent is lumen's own light: the line under the active tab, the match a search is on, the frame of the search field. It is the one color in this table that Alacritty doesn't have. A theme you paste won't set it, so set it yourself to a color that goes with the theme.

### Bold text

```toml
[colors]
draw_bold_text_with_bright_colors = false
```

Set to `true`, bold text in one of the eight normal colors is drawn in its bright version, as some older terminals do.

## Colors a program sets

A program can change colors while it runs: any of the sixteen and the 256, the text, the cursor and the background. Scripts that switch a terminal to another theme, such as base16-shell, do just that.

lumen follows, in the pane the program runs in. Other panes and tabs keep the theme, and so does lumen's own frame: the tabs and the title bar.

![Two panes side by side in lumen, with the same lines in both. The left one has the theme's dark background. In the right one a script has set a light background and dark text](/shots/light-pane.png)

A background that differs from the theme's fills the whole pane, and it is solid, whatever `opacity` says.

A cursor color that a program sets is the cursor's. Where a program sets a background and no cursor, the theme's cursor stays as long as it stands out from that background. If it doesn't, as a light cursor on a light background, the cursor takes the color of the text. The selection on such a background is a wash of the text's color, whatever the theme says for it: the theme's colors were chosen for the theme's background.

The colors stay until a program sets them back, or the pane is closed. [Troubleshooting](/docs/troubleshooting/#a-pane-has-other-colors-than-the-theme) has the line that sets them back by hand.

## A see-through window

How solid the background is, is not a color. It is `opacity` in the [`[window]`](/docs/configuration/#window) table.
