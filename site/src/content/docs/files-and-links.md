---
title: Files and links
description: A name in a listing is the file. Click it to copy it, drag it to take it along.
---

When you look at a listing in a terminal, you see file names. To do something with one of the files outside the terminal (attach it to a mail, drop it into a chat, move it to another folder) you normally open a file manager and find it again.

In lumen the name is the file.

## What a file name does

| On a file name | Does |
| --- | --- |
| Click | Copies the file, as a file manager's Copy does |
| `Ctrl+Click` | Opens it in its program |
| Drag | Takes the file along |

**A click copies the file**, not its name. Paste in a file manager and you get a copy of the file. Paste into a mail or a chat and it is attached. lumen says so at the bottom of the window: `Copied notes.md`.

**`Ctrl+Click` opens** the file the way a double click in a file manager does: a picture in your image viewer, a folder in your file manager.

**Dragging** a name takes the file with it. Drag it onto a pane, and its path is typed there, quoted for the shell. On Wayland you can also drag it out of the window and drop it into another program.

When the mouse is on a name that can be clicked, the name is underlined.

## Which names are files

lumen does not guess. A name is a file when the program that printed it says so, with a link around the name. [glint](https://github.com/Vaspyyy/glint) does that when you ask it to:

```sh
glint --hyperlink=copy
```

Or once and for all, in `~/.config/glint/config.toml`:

```toml
hyperlink = "copy"
```

glint 0.11.0 or newer is needed. These links work in other terminals as well, where a click on them starts glint to do the copying. In lumen nothing has to be started.

## Other links

Programs can print links of any kind: web pages, or the `file://` links that `ls --hyperlink` and many other tools print. In lumen they open with `Ctrl+Click`: a web page in your browser, a file in its program.

A plain click on these does nothing, so that you can still select the text. Only a file name from a listing is copied by a plain click.

> A web address that is printed as plain text, with no link around it, is not clickable yet. lumen follows the links programs mark as links.

## Selecting text that is a link

Hold `Shift`. With `Shift` down, a click or a drag on a link selects text as it does everywhere else.

## Files from elsewhere

Drop files from a file manager onto a pane, and their paths are typed there, quoted for the shell and separated by spaces. That is the quick way to hand a file to a command.

## What it needs

Copying a file needs a small helper to hold the clipboard:

- on Wayland: `wl-copy`, from the package `wl-clipboard`
- on X11: `xclip`

Without it, lumen tells you which one is missing when you click a name.

## What lumen checks

A link can come from anywhere: from a file you print, from a server you are logged in to. So lumen is careful with them.

- **A click copies only what the text names.** If a link's text says one thing and it leads to another file, a plain click does not copy it. It is treated like any other link, and needs `Ctrl`.
- **A link to a file on another machine is not followed.** A listing printed over SSH names files on the server. lumen sees that, and does not mix them up with files on your machine.
- **A file that is gone** is not copied, and lumen says that it is gone.

## Not there yet

- Dragging a file out of the window works on Wayland, not on X11.
- Several files can't be selected and copied together.
