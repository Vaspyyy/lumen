---
title: Troubleshooting
description: The things that go wrong most easily, and what to do about them.
---

## lumen doesn't start

Start it from another terminal, and read what it prints. `--new-process` makes it a lumen of its own, so that what it prints is not about the one that runs already:

```sh
lumen --new-process
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
- **bash has the line and still shows no marks**: a theme that builds the prompt before every prompt was added after lumen's line. It then runs after lumen's part and takes the marks out again each time. Make lumen's line the last one in `~/.bashrc`, after the prompt's theme.
- **bash shows no marks, and its tab looks busy while nothing runs**: both the prompt and `PROMPT_COMMAND` were set anew after lumen's line, in the file or by hand. Nothing of lumen's is left then but the mark for a command's start. Read `~/.bashrc` again with lumen's line as its last one: it puts its part back.
- **`padding` is under 4** in the config: there is no room for the marks. The keys still work.
- **Inside tmux** or another multiplexer the marks don't reach lumen.

## `Ctrl+Click` on a path does nothing

Hold `Ctrl` and look whether the path is underlined. If it is not, lumen found no such file:

- The path is meant from another folder than lumen looked in. With a shell that [marks its commands](/docs/commands/), lumen knows where each command ran. Without, it goes by where the shell is now.
- The path is on another machine, in a pane where you are logged in over SSH.
- The path has a space in it, or is broken over two lines.

If it opens in the wrong program, say which editor you want: [`[editor]`](/docs/configuration/#editor) in the config.

## There is no light in the title bar

The light and the numbers on tabs come from [pulse](/docs/machine/), which has to be installed: `go install github.com/Vaspyyy/pulse@latest`. Where it is installed and nothing shows:

- lumen has to find it: on the `PATH` lumen was started with, or in `~/go/bin` or `~/.local/bin`.
- `enabled` in the `[pulse]` table of the config must not be `false`.
- lumen looks for pulse every half minute until it finds it. One that was installed just now shows within that time.
- A pulse that is too old to know `pulse watch` is started and ends at once. lumen tries three times, 20 seconds apart, and then leaves it alone until lumen is started again or the `[pulse]` table changes.

## A pane has other colors than the theme

A program can change a pane's colors while it runs, its background too. If one has ended and left its colors behind, this sets them back to the theme's:

```sh
printf '\033]104\a\033]110\a\033]111\a\033]112\a'
```

A new tab or pane has the theme's colors again in any case. See [Colors](/docs/colors/#colors-a-program-sets).

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

lumen keeps a few keys for itself, listed in [Keys](/docs/keys/). Everything else goes to the program. To give one of lumen's back, name it in `[keys]` with `"none"`: `"ctrl-shift-t" = "none"`. See [Changing the keys](/docs/keys/#changing-the-keys).

## There is empty space around the window

Where lumen draws its own frame, it keeps a margin around the window for its shadow, and rounds its corners. A desktop says when a window is tiled, and lumen then leaves both out; some tiling desktops don't say so. `decorations = "none"` in the `[window]` table takes the margin, the corners and the border away for good, and `decorations = "desktop"` lets a desktop that draws frames draw one.

Inside each pane, `padding` is the space around the text: `padding = 0`, or `padding = { x = 4, y = 0 }`. See [Configuration](/docs/configuration/#window).

## A new tab opens in the wrong folder

A new tab starts where the shell of the current pane is. If that shell has started another program that changed folders on its own, or you are logged in to another machine in that pane, lumen sees only where the local shell is.

## Starting lumen opens a tab, not a window

That is what it does while a lumen runs: a new tab in the window you used last, brought to the front. If that window is on another workspace, the compositor may show you that workspace, or only mark the window. `launch = "window"` in the config opens windows instead, and `launch = "process"` a lumen of its own every time. See [Opening lumen again](/docs/launching/).

## Something else

Look through the [open issues](https://github.com/Vaspyyy/lumen/issues), and open a new one if yours isn't there. Say which version you have (`lumen --version`), your desktop, and whether it is Wayland or X11.
