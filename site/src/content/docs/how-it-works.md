---
title: How it works
description: What lumen is built from, and the two ideas in it that are its own.
---

lumen is written in Rust, and stands on two parts that have proven themselves in other programs.

- **The terminal itself** is [alacritty_terminal](https://crates.io/crates/alacritty_terminal), the engine of the Alacritty terminal: it understands escape sequences, and keeps the grid of cells and the scrollback.
- **The window and everything drawn in it** is [GPUI](https://github.com/longbridge/gpui-fast), the UI framework of the Zed editor. It draws with the graphics card, on Wayland and on X11.

What lumen adds is everything between the two: tabs and splits, the keyboard and the mouse, links, pictures, search, the config file.

## A screen that is drawn every frame

GPUI redraws the window whenever something changes. lumen lays out the cells of each visible pane: backgrounds and the marks of a search first, then pictures, then text, then the line under a link, and last the cursor and its light.

The cursor's glow and glide are done there. The cursor is not a cell that is filled in: it is a shape with a position of its own, which moves towards the cell it belongs in.

## The thread that listens

A program in a terminal writes to a *PTY*, a pipe that behaves like a terminal. One thread per pane reads from it and hands what it reads to the engine, which updates the grid. The window is told that there is something new and draws the grid when it gets to it. A program that prints faster than the screen can show loses no time over that.

That thread is lumen's own, for one reason: pictures.

## Pictures as cells

Pictures arrive in escape sequences of a kind the engine drops. So lumen looks at what a program prints before the engine does, takes the picture commands out, and passes the rest on untouched.

The harder question is what a picture *is* on a grid of characters. It has to scroll with the text, disappear when the screen is cleared, and be covered when something is printed over it. Teaching the engine all of that would mean rewriting it.

lumen does something simpler. A picture occupies cells, as text does. lumen marks each of those cells with a link of its own kind that names the picture and the part of it that belongs in that cell. The engine knows links: it scrolls them, erases them and overwrites them along with the cells they are on. It moves pictures around without knowing what one is.

When a pane is drawn, the cells with such a mark are not drawn as text. The drawing code looks the picture up by the mark and draws that part of it.

When no cell names a picture any more, because it has scrolled out of the scrollback or was cleared, lumen frees it the next time it tidies up. A picture a program gave a number to is kept for a while longer, so that the program can show it again.

## Names as files

File names work the other way around. Here the program that prints a listing does the marking: it puts an ordinary link around each name. lumen reads the link under the mouse, sees that it leads to a file on this machine, and treats the cells it covers as that file.

Nothing is guessed from the text. A path that merely looks like a file name is text, and a name is a file only because the program said so.

## A shell that waits

With [`prewarm`](/docs/instant-tabs/) on, a window keeps one terminal that is not shown: a shell that has already started. A new tab takes it and shows it, and the next one is started behind the scenes.

## One lumen for every window

Usually a user has one lumen at a time. The first one listens on a socket, `lumen.sock` in `$XDG_RUNTIME_DIR/lumen`, that only the same user may connect to. A `lumen` started while it runs connects before it does anything else, before the GPU, the fonts or the config: it sends the folder, the program, its environment, its display and the launcher's focus token, and the running lumen opens the tab or the window. The launch ends a few milliseconds after it started, or, with `-e`, when the tab's program ends. If nothing answers, or the answer is no (another version, another display), it starts as a lumen of its own.

`lumen --daemon` is such a lumen without a window, started at login. It has no display until a launch names one, and when its last window closes it lets go of the display again and starts a GPU device for the next window. See [Opening lumen again](/docs/launching/).

## Where things are

```
src/
  main.rs       command line, window
  instance.rs   one lumen for all windows: launches and the daemon
  update.rs     lumen update: lumen and its tools at their newest releases
  config.rs     the config file
  terminal.rs   a running terminal: the program, its PTY, its grid
  pty.rs        the thread that reads what the program prints
  graphics.rs   pictures: the kitty graphics protocol
  element.rs    draws the grid
  view.rs       a terminal on screen: keyboard, mouse, selection, links, search
  links.rs      what a link leads to, and copying files
  keys.rs       what a key press sends to the program
  mouse.rs      mouse reports for programs that ask for them
  palette.rs    from the colors a program asks for to the theme's
  workspace.rs  tabs and splits
  actions.rs    everything a key can be bound to
```

The whole program is about 6,000 lines, and is meant to be read. See [Contributing](/docs/contributing/).
