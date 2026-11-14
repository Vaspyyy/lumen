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

lumen then uses your desktop's fixed-width font, and prints a line that says so to the terminal it was started from. On a system without any fixed-width font it takes a font that is there, and the letters may not line up.

It does not take the nearest font the system offers. That would often be a font whose letters have different widths, and a terminal's grid does not survive that.

## Size and line height

`size` is in points, as in other terminals and in your desktop's settings, so 12 in lumen is the 12 you know. It can be anything from 4 to 96.

`line_height` is the height of a row as a multiple of the font's own line height. `1.0` is what the font's designer chose. `1.2` gives the lines more air, and fits fewer of them into the window.

## Ligatures and weight

```toml
[font]
ligatures = false
weight = 300
bold_weight = 600
```

Some fonts join letters into one sign: `->` into an arrow, `!=` into a crossed equals sign. lumen draws those, as the font intends. `ligatures = false` keeps every letter as it is.

`weight` is how heavy plain text is, and `bold_weight` how heavy bold text is, from 100 (thin) to 900 (black); 400 is regular and 700 bold. A font that doesn't have the weight you ask for is drawn in the nearest one it has. A light weight suits a light theme or a screen with many pixels.

## Spacing, and lumen's own words

```toml
[font]
letter_spacing = 1
ui_family = "Inter"
ui_size = 13
```

`letter_spacing` puts that many pixels between two characters, beyond what the font has, from -5 to 20. Every cell gets wider by it, so the grid stays a grid.

`ui_family` and `ui_size` are for lumen's own words: the tab titles, the notes, the search field. Unset, the family is your desktop's font, the one other programs use for their menus. A family that can't be found is your desktop's too, and lumen prints a line that says so.

## Size for one pane

`Ctrl+Plus` and `Ctrl+Minus` change the text size of the pane you are in, and `Ctrl+0` brings it back. The other panes keep theirs. That is for the moment something has to be read from across the room, or a wide table has to fit.

## Characters the font doesn't have

No font has every character. Where yours has none, such as for an emoji or for a script it doesn't cover, lumen draws the character with another font from your system that has it.

## Characters lumen draws itself

The block characters, from `▀` to `▟` (U+2580 to U+259F), do not come from the font. Programs build bars, charts, QR codes and coarse pictures from them, and for that each has to fill its part of the cell exactly. A font's shapes don't: they are made for the font's own line and are soft at the edges, which leaves thin lines between them. lumen fills these characters itself, on the cell's edges, with every font and at every size. The three shades, `░`, `▒` and `▓`, are an even quarter, half and three quarters of the text color.

The characters that draw lines, such as `─` and `│`, come from the font.

## Icons

Tools such as [glint](https://github.com/Vaspyyy/glint) draw icons next to file names. Those icons are characters from a [Nerd Font](https://www.nerdfonts.com/). Install one, as your terminal font or just next to it, and they appear. Without one you see empty boxes where the icons would be, and glint can be told to leave them out with `--icons never`.
