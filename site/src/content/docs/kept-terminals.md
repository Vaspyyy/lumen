---
title: Terminals that keep running
nav: Kept terminals
description: A terminal the PC keeps running while nothing shows it, so you can leave and come back.
---

A terminal lives as long as its window. Close the window, lose the connection, and the shell in it is gone with everything it was doing.

A kept terminal is different: the PC runs it, and a window only shows it. This is what the [phone app](/docs/phone/) opens, and you can use it without a phone:

```sh
lumen attach --session work
```

The first time, this makes a terminal called `work` with your shell in it. Close the window, and the shell runs on. Say the same command again, in another window, over SSH from another machine, or by opening the tab on your phone, and the terminal is back: what was on the screen, what scrolled past, the program that was running.

## One place at a time

A kept terminal is shown in one place. When it is opened somewhere else, the place that showed it before lets go and says so:

```
lumen: this terminal is now shown somewhere else
```

So a build you started at the desk can be picked up on the phone, and taken back at the desk.

## What comes back

- The screen as it was, with its colors.
- Up to 5000 lines of what scrolled past.
- The cursor, the title, and what the program had switched on, such as the mouse.
- A program that has the whole screen, such as an editor, is told that the window changed, and draws itself again.

The terminal takes on the size of the window that shows it.

Not kept: [pictures](/docs/images/). A picture is drawn while you watch, and is not there when you come back.

## Names

A name is made of letters, digits, `-`, `_` and `.`, at most 64 of them, and starts with a letter, a digit or `_`.

## List them, end them

```sh
lumen sessions
```

```
NAME   ATTACHED  STARTED  FOLDER      TITLE
work   yes       09:12    ~/site      nvim
build  no        10:40    ~/site/app  cargo build
```

`ATTACHED` says whether something shows the terminal right now.

A kept terminal ends when its shell ends: type `exit`, as in any terminal. From outside:

```sh
lumen sessions kill build     # one
lumen sessions kill --all     # all of them
```

This hangs up on the shell, the way closing a window does, so what runs in it ends too. A program that was started to outlive its terminal, with `nohup` or `setsid`, can stay.

## What the shell finds

A kept terminal starts your login shell, in your home folder. `--cwd` names another folder, `--env` sets a variable:

```sh
lumen attach --session site --cwd ~/Projects/site --env EDITOR=nvim
```

Both count only when the terminal is made. For a terminal that is running, they are ignored.

| Variable | Value |
| --- | --- |
| `TERM` | `xterm-256color` |
| `COLORTERM` | `truecolor` |
| `LUMEN_SESSION` | The terminal's name, such as `work` |

## When you log out

The terminals are kept by one small process per user, `lumen keeper`. The first `lumen attach` starts it, and it ends with the last terminal.

On most systems, the programs of a user are ended when that user's last login ends. For a PC you sit at, that is when you log out of the desktop. For a machine you only reach over SSH, it is each time you disconnect, unless you tell the system to let your programs run:

```sh
loginctl enable-linger
```

`lumen pair` says so when it finds this off.

A restart of the PC ends every terminal, kept or not.

## Limits

- **A lost connection is noticed late.** When a phone drops off the network without saying goodbye, `ATTACHED` can say `yes` for a while. Opening the terminal again takes it over, so nothing is in the way.
- **Some of a program's settings are not carried over.** A scroll region, tab stops and colors a program redefined are forgotten when a terminal is shown again. A program that draws the whole screen sets them again itself.
- **No pictures**, as said above.
