---
title: Looks
description: Every part of how lumen looks can be changed: the frame, a picture behind the text, the tab bar, the panes, the cursor, and lumen's own colors.
---

Everything on this page goes into the [config file](/docs/configuration/), and saving it is enough: the window changes while you look at it. Most of it is in the [settings menu](/docs/configuration/#the-settings-menu) too. The colors of the text are on [Colors](/docs/colors/), the fonts on [Fonts](/docs/fonts/).

## The window's frame

Where lumen draws its own frame, it has round corners, a thin border and a shadow:

```toml
[window]
corner_radius = 12      # how round the corners are, in pixels, up to 40: 0 is square
border = true           # the thin line around the window
shadow = true           # the shadow around it
```

The shadow is drawn around the window only, never under it, so a see-through window is as see-through as `opacity` says. Everything in the window keeps to its round corners, so very round ones cut into the text's own corners: a `padding` of about a third of `corner_radius` keeps the text clear of them. `decorations = "desktop"` gives your desktop's own frame instead, and `"none"` no frame at all. See [Configuration](/docs/configuration/#window).

## A picture behind the text

```toml
[window]
background_image = "~/Pictures/night.jpg"
background_image_opacity = 0.3   # how much of the picture shows: 1 is all of it
background_image_fit = "cover"   # cover, contain, fill
```

The picture lies over the background's own color, and the text over it. With `background_image_opacity` low, it is a hint behind the text; at 1 it is all there is. `cover` fills the window and cuts off what sticks out, `contain` shows all of the picture with room beside it, `fill` stretches it to the window's shape.

PNG, JPEG, WebP, SVG and the other usual kinds work, and a GIF that moves keeps moving (lumen then draws all the time, which costs a little power). `~` is your home folder, and a path that doesn't start at `/` or `~` starts in the folder `lumen.toml` is in: `background_image = "wall.png"` is a picture beside it. In the [settings menu](/docs/configuration/#the-settings-menu), a picture is typed in from `/` or `~/`, and lumen checks that it is there and is a picture.

The picture is part of the background, so on a see-through window (`opacity` below 1) it is see-through too: its strength is `background_image_opacity` times `opacity`.

## A gradient

```toml
[window]
gradient = "#3a1030"    # the background fades from its own color into this one
gradient_angle = 180    # which way: 180 from the top down, 90 from the left
```

The background's own color is `background` in [`[colors.primary]`](/docs/colors/). A gradient goes under a picture, so the two can go together.

## The tab bar

```toml
[tabs]
bar = "always"          # always; auto: from two tabs on; never
position = "top"        # top or bottom
```

## Panes

```toml
[panes]
dim_inactive = 0.22     # how far the panes you aren't typing in step back: 0 not at all
divider_width = 1       # the line between two panes, in pixels: 0 is none
```

## The cursor

```toml
[cursor]
glow = true
glow_strength = 1.0     # how bright its light is: 0 is none, 3 the most
glow_size = 1.0         # how far the light reaches
smooth = true
glide_time = 70         # milliseconds it takes to glide to its next cell
blink = true
blink_interval = 530    # milliseconds lit, and as many dimmed
blink_dim = 0.22        # how bright it is at its dimmest: 0 goes out, as in other terminals
blink_fade = 140        # milliseconds to dim and to come back: 0 is at once
rounded = true
```

`style`, `thickness` and `unfocused` are on [Configuration](/docs/configuration/#cursor).

## Letters

```toml
[font]
letter_spacing = 0      # pixels between two characters, beyond the font's own
ui_family = "Inter"     # the font of lumen's own words: the tabs, the notes, the search
ui_size = 12.5          # their size, in pixels
```

`ui_family` unset is your desktop's font. More in [Fonts](/docs/fonts/).

## lumen's own colors

The tabs, the frame, the lines between panes and the notes are drawn in `[colors.ui]`. Each color can be see-through: `#rrggbbaa`, where `aa` is how much of what is behind it it covers, from `00` (nothing) to `ff` (all of it).

```toml
[colors.ui]
text = "#ffd6f0"          # lumen's own words and signs
border = "#ff7ac860"      # the line around the window
divider = "#ff7ac8aa"     # the line between two panes
tab_bar = "#00000066"     # behind the tab bar; the window's background if unset
active_tab = "#ff7ac840"  # behind the tab you are in
shadow = "#00000059"      # the window's shadow
```

None of them has to be set. `text` unset is white on a dark background and black on a light one, so a light theme works without it, and the others unset are shades of `text`, but for the shadow, which is black. The line under the active tab is in the `accent` color of [`[colors]`](/docs/colors/#the-accent), and the cursor's glow in the cursor's own color.

## Looks to start from

**Glass.** The desktop shows through, blurred where it can be:

```toml
[window]
opacity = 0.6
blur = true

[colors.ui]
tab_bar = "#00000040"
```

**A picture.** A wallpaper of its own behind the text:

```toml
[window]
background_image = "~/Pictures/wallpaper.png"
background_image_opacity = 0.25
corner_radius = 18

[tabs]
position = "bottom"
```

**Nothing but text.** For a tiling desktop, or for anyone who wants only the terminal:

```toml
[window]
decorations = "none"
padding = 4

[tabs]
bar = "auto"

[panes]
dim_inactive = 0
divider_width = 1

[cursor]
rounded = false
glow = false
smooth = false
blink_dim = 0
blink_fade = 0
```

**Neon.** A strong glow and colored lines:

```toml
[colors]
accent = "#ff4fd8"

[colors.cursor]
cursor = "#ff4fd8"

[colors.ui]
divider = "#ff4fd8"
border = "#ff4fd880"

[cursor]
glow_strength = 2.5
glow_size = 2

[panes]
divider_width = 2
```
