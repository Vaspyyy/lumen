---
title: Roadmap
description: What is done, what comes next, and what is missing.
---

lumen grows in steps. Each one makes it understand more of what is on the screen.

## 1. Daily driver

**Done**, in [0.1.0](/releases/0.1.0/).

A terminal you can live in. Tabs, splits, selection, scrollback, your fonts and colors.

## 2. Files and pictures

**Done**, in [0.2.0](/releases/0.2.0/).

Names in a listing are files: click to copy, `Ctrl+Click` to open, drag one out into another program. Pictures in place, through the kitty graphics protocol. Search through the scrollback.

## 3. Command blocks and refs

**Next.**

Today a terminal's history is one long stream of text. Shells can mark where each command starts, where its output ends and how it ended. With those marks lumen can treat a command and its output as one thing:

- jump from command to command
- copy the output of one command
- see at a glance which commands failed

The second half is about places in code. Tools such as crux, seek and recap print them as `path:line`. In lumen a click on one will open that place in your editor.

## What is missing

Things other terminals have and lumen doesn't, in no order. None of them has a date.

- **Keys can't be changed.** The keys lumen takes are fixed.
- **No packages.** lumen is built from source. There is nothing in a distribution's repositories yet.
- **Linux only.**
- **Web addresses in plain text are not clickable.** Only what a program marks as a link is.
- **Dragging a file out of the window** works on Wayland, not on X11.
- **Animated pictures** show their first frame.
- **Sixel pictures** are not drawn.
- **The bell** does nothing.

If one of these keeps you from using lumen, [say so](https://github.com/Vaspyyy/lumen/issues). What people miss most is done first.
