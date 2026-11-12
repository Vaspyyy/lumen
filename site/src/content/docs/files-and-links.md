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

## Web addresses and places in code

Most of what you would like to click is not marked as a link at all: a web address in a log, the `src/main.rs:42:9` in a compiler's error. lumen finds those in the text.

Hold `Ctrl` and move the mouse onto one. If lumen can follow it, it is underlined, and a click does:

| `Ctrl+Click` on | Does |
| --- | --- |
| A web address, such as `https://example.com/docs` | Opens it in your browser |
| A file or folder that is there, such as `notes/todo.md` | Opens it in its program |
| A file with a line, such as `src/main.rs:42` | Opens the file at that line in [your editor](#your-editor) |

lumen reads a line in these forms:

```
src/main.rs:42            src/main.rs:42:9
src/main.rs(42)           src/main.rs(42,9)
File "tools/build.py", line 42
```

Those cover what compilers, linters, test runners and searches print, among them crux, seek and recap. A full stop, a comma or a bracket around an address or a path is left out.

**Only a file that is there is a link.** lumen looks on the disk before it underlines a path, so a word with a dot in it does not turn into a link by chance. It looks only while `Ctrl` is held.

**Where a path is looked for.** A path that starts with `/` or `~` is taken as it is. Any other is followed from the folder the command that printed it ran in, which lumen knows when [your shell marks its commands](/docs/commands/); without that, from the folder the pane's shell is in now. Tools that run further down in a project often name files from its top, so for a path with a `/` in it the five folders above are tried as well, up to your home folder. A bare name such as `main.rs` is looked for in that one folder only.

Not found this way: a path that is broken over two lines, and a path with a space in it.

## Your editor

A file with a line opens in your editor, at that line. Which program that is:

1. `command` in the `[editor]` table of the [config](/docs/configuration/#editor), if you set it
2. else what `$VISUAL` names, else `$EDITOR`
3. else the program your desktop opens the file with, which does not know about the line

```toml
[editor]
command = "nvim"
```

An editor that runs in a terminal, such as vim, nvim, helix, nano or micro, opens in a new tab, in the file's folder. The tab closes when you leave the editor. An editor with a window of its own, such as VS Code, Zed, Kate or a JetBrains IDE, is started beside lumen.

lumen knows how the common editors take a line. For any other, write the whole command, with `{file}`, `{line}` and `{column}` where they belong, and say whether it runs in a terminal:

```toml
[editor]
command = "myeditor --line {line} --column {column} {file}"
terminal = false
```

`$VISUAL` and `$EDITOR` are read from the environment lumen was started in. Started from the application menu, that is your desktop's, which may not have what your shell's startup file sets. If lumen opens the wrong program, set `command`.

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
- A path over SSH names a file on the other machine. lumen looks for it on yours, and finds it only if the same file is there too.
- Several files can't be selected and copied together.
