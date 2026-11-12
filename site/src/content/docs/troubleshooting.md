---
title: Troubleshooting
description: The things that go wrong most easily, and what to do about them.
---

## lumen doesn't start

Start it from another terminal, and read what it prints:

```sh
lumen
```

- **A message about Vulkan or a graphics adapter.** lumen is drawn by the graphics card and needs a working driver. `vulkaninfo --summary` (from the package `vulkan-tools`) shows whether Vulkan works on your system.
- **`lumen: command not found`.** The folder the program is in has to be on your `PATH`. Installed with the install script, that is `~/.local/bin`; built from source, it is `~/.cargo/bin`, which rustup normally adds (log out and in again after installing Rust). A package puts it into `/usr/bin`, which always is.
- **A message about a library that can't be found, or about a graphics adapter.** lumen was installed without a package, and the system lacks one of the libraries it needs. [Install](/docs/install/#any-other-distribution) lists them.
- **A message that there is no font.** lumen needs one installed font to draw text with, and found none. Install any, DejaVu for example.
- **A message about the config file.** lumen starts anyway, with its defaults, and the message says what is wrong in the file.

## The font is not the one I set

lumen prints a line when the font in your config isn't installed, and uses your desktop's fixed-width font instead. Check the name:

```sh
fc-list :spacing=mono family | sort -u
```

See [Fonts](/docs/fonts/).

## Icons are empty boxes

The icons that tools such as glint draw are characters from a [Nerd Font](https://www.nerdfonts.com/). Install one. It doesn't have to be your terminal font.

## A click on a file name does nothing

Move the mouse onto the name first. If it is **not underlined**, lumen doesn't know it is a file:

- The listing has to come from [glint](https://github.com/Vaspyyy/glint) 0.11.0 or newer, with `hyperlink = "copy"` in its config or `--hyperlink=copy` on the command line.
- A program that uses the mouse itself, such as a file manager or an editor, gets the click. Hold `Shift` and the mouse is lumen's.
- A listing printed over SSH names files on the other machine. lumen doesn't copy those.

If it **is underlined** and the click copies nothing, lumen says why at the bottom of the window. Usually the helper that holds the clipboard is missing: install `wl-clipboard` on Wayland, `xclip` on X11.

## Commands have no marks

The marks beside commands, the keys that jump between them and the dots on tabs all need a shell that says where its commands are.

- **bash and zsh** need one line in their startup file, and **fish** needs version 4.0 or the same kind of line. See [Commands](/docs/commands/#setting-up-your-shell). The line counts for tabs opened after it was added.
- **`padding` is under 4** in the config: there is no room for the marks. The keys still work.
- **Inside tmux** or another multiplexer the marks don't reach lumen.

## `Ctrl+Click` on a path does nothing

Hold `Ctrl` and look whether the path is underlined. If it is not, lumen found no such file:

- The path is meant from another folder than lumen looked in. With a shell that [marks its commands](/docs/commands/), lumen knows where each command ran. Without, it goes by where the shell is now.
- The path is on another machine, in a pane where you are logged in over SSH.
- The path has a space in it, or is broken over two lines.

If it opens in the wrong program, say which editor you want: [`[editor]`](/docs/configuration/#editor) in the config.

## `glint --thumbs` draws blocks, not pictures

- glint is older than 0.11.1 and doesn't know lumen. Update it, or run `glint --thumbs=kitty`.
- You are inside tmux or another multiplexer, which doesn't pass pictures on by default.
- You are logged in to another machine. glint there can't see which terminal you use; `--thumbs=kitty` tells it.

## A change to the config did nothing

- Look at the bottom of the window when you save. `Config reloaded` means lumen took it. Anything else is what is wrong with the file.
- Make sure it is the right file: `lumen --config-path`.
- `columns` and `lines` set the size of the *next* window, and `shell` the program of the next tab.

## The window is not see-through

`opacity` below 1 needs a desktop that mixes windows together, which Wayland desktops and most X11 ones do. Blur behind the window is up to the desktop: KDE Plasma does it, others may not, and lumen is then see-through without blur.

## A key doesn't reach my program

lumen keeps a few keys for itself, listed in [Keys](/docs/keys/). They can't be changed yet. Everything else goes to the program.

## A new tab opens in the wrong folder

A new tab starts where the shell of the current pane is. If that shell has started another program that changed folders on its own, or you are logged in to another machine in that pane, lumen sees only where the local shell is.

## Something else

Look through the [open issues](https://github.com/Vaspyyy/lumen/issues), and open a new one if yours isn't there. Say which version you have (`lumen --version`), your desktop, and whether it is Wayland or X11.
