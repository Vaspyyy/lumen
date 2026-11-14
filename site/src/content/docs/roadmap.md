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

## On the side: your phone

**Done**, in [0.3.0](/releases/0.3.0/).

An app for Android that opens terminals on your PC. They keep running there, so what you start at the desk can be picked up on the phone and taken back. The phone logs in over SSH with a key in its secure hardware. See [lumen on your phone](/docs/phone/).

## 3. Command blocks and refs

**Done**, in [0.4.0](/releases/0.4.0/).

A terminal's history used to be one long stream of text. Shells can mark where each command starts, where its output ends and how it ended. With those marks lumen treats a command and its output as one thing: you jump from command to command, copy the output of one, and see at a glance which ones failed. A tab says when a command in it has ended. See [Commands](/docs/commands/).

The second half is about places in code. Tools such as crux, seek and recap print them as `path:line`. A `Ctrl+Click` on one opens that place in your editor. See [Files and links](/docs/files-and-links/#web-addresses-and-places-in-code).

## 4. The machine in view

**Done**, in [0.5.0](/releases/0.5.0/).

[pulse](https://github.com/Vaspyyy/pulse), a tool of the family, knows what the machine is busy with. lumen shows that where you work: a light in the title bar when the machine is busy, what each tab costs beside its name, and a notification when a long run in another tab is over. See [The machine](/docs/machine/).

## 5. What a command really cost

**Next.**

The memory beside a command's mark is what was seen every few seconds. The system can count instead: with each pane in a group of its own, a command's CPU time and its peak memory are exact, and are there for every command, however short.

## What is missing

Things other terminals have and lumen doesn't, in no order. None of them has a date.

- **Not in any distribution's repositories.** lumen has [packages of its own](/docs/install/), which a system's updates don't know about: a new version is installed by hand.
- **Packages for one kind of machine.** They are built for `x86_64`. On ARM, lumen is built from source.
- **Linux only**, and Android for the phone app. No iPhone.
- **The phone app shows no pictures**, has no splits and no search, and doesn't take your font and colors from the PC.
- **Dragging a file out of the window** works on Wayland, not on X11.
- **Animated pictures** show their first frame.
- **Sixel pictures** are not drawn.
- **The bell** makes no sound. It puts a dot on a tab you are not looking at.
- **Folding a command's output away.** lumen knows where a command's output is, and can't hide it yet.

If one of these keeps you from using lumen, [say so](https://github.com/Vaspyyy/lumen/issues). What people miss most is done first.
