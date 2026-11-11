---
title: Pictures
nav: Pictures
description: Pictures a program sends are drawn in place, and scroll with the text.
---

lumen draws pictures between the lines of text. They take up cells as text does, so they scroll with it, are cleared with it, and are still there when you scroll back.

![The thumbnails of a folder's pictures in lumen, made by glint --thumbs](/shots/thumbs.png)

## Try it

With [glint](https://github.com/Vaspyyy/glint) 0.11.1 or newer, in a folder with images:

```sh
glint --thumbs
```

With kitty's `icat`, if you have kitty installed:

```sh
kitten icat photo.jpg
```

Both have been tested with lumen. Pictures are sent with the [kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/), so other programs that speak it should work too.

## What lumen understands

| | |
| --- | --- |
| Formats | PNG, and raw pixels with and without transparency (RGBA and RGB), compressed or not |
| Ways to send | In the escape sequence itself, in pieces; or as a file the program names; or as a temporary file, named as the protocol says, that lumen removes after reading |
| Size | At the picture's own size; or fitted to a number of columns or of rows, keeping its proportions; or stretched over both |
| Part of a picture | A program can show a cut-out of a picture it sent |
| Showing again | A picture sent once can be shown several times, moved, and removed |
| Answers | lumen answers a program that asks whether pictures work |

## Limits

- A pane keeps up to **320 MB** of pictures. When there are more, the oldest go first.
- A picture larger than **4096 pixels** on a side is scaled down to that. Nothing on a screen is lost by it.
- Pictures that have scrolled out of the scrollback, or were cleared from the screen, are freed when lumen next tidies up. Pictures a program gave a number to, to show them again, are kept up to 160 MB.
- A picture larger than 16,384 pixels on a side, or than 64 million pixels in all, is refused.

## Not there yet

- **Animations.** Only the first frame of a moving picture is shown.
- **Placement by placeholder characters**, which some programs use to put pictures into other programs' text.
- **Shared memory** as a way to send. lumen tells a program that asks, and kitty's `icat` then sends a file instead.
- **Other picture protocols**: Sixel and iTerm2's.

## How a program knows

lumen sets `TERM_PROGRAM=lumen`. A program can also ask: lumen answers the query the kitty graphics protocol defines for that. See [For programs](/docs/for-programs/).

## Files a program names

A program can ask lumen to read a picture from a file. lumen reads only ordinary files, and nothing from `/proc`, `/sys` or `/dev`, except `/dev/shm`, where programs put temporary pictures. It follows links before it checks, so a link can't lead around that.
