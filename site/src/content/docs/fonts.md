---
title: Fonts
description: Which font lumen draws with, how big, and what happens when a font is missing.
---

```toml
[font]
family = "Maple Mono"
size = 12.0
line_height = 1.0
```

## The family

`family` is the name of a font installed on your system. To see the names:

```sh
fc-list :spacing=mono family | sort -u
```

Upper and lower case and spaces don't matter: `MapleMono` and `maple mono` both mean Maple Mono.

`"monospace"`, the default, is not a font of its own: it is whatever fixed-width font your desktop is set to.

## When the font is not installed

lumen then uses your desktop's fixed-width font, and prints a line that says so to the terminal it was started from.

It does not take the nearest font the system offers. That would often be a font whose letters have different widths, and a terminal's grid does not survive that.

## Size and line height

`size` is in points, as in other terminals and in your desktop's settings, so 12 in lumen is the 12 you know. It can be anything from 4 to 96.

`line_height` is the height of a row as a multiple of the font's own line height. `1.0` is what the font's designer chose. `1.2` gives the lines more air, and fits fewer of them into the window.

## Size for one pane

`Ctrl+Plus` and `Ctrl+Minus` change the text size of the pane you are in, and `Ctrl+0` brings it back. The other panes keep theirs. That is for the moment something has to be read from across the room, or a wide table has to fit.

## Characters the font doesn't have

No font has every character. Where yours has none, such as for an emoji or for a script it doesn't cover, lumen draws the character with another font from your system that has it.

## Icons

Tools such as [glint](https://github.com/Vaspyyy/glint) draw icons next to file names. Those icons are characters from a [Nerd Font](https://www.nerdfonts.com/). Install one, as your terminal font or just next to it, and they appear. Without one you see empty boxes where the icons would be, and glint can be told to leave them out with `--icons never`.
