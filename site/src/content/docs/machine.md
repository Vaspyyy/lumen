---
title: The machine
description: What the machine is busy with, and what each tab costs it. Shown where pulse is installed.
---

A build is slow, the fans are loud, a game stutters: something is using the machine, and the terminal you are looking at usually knows nothing about it. lumen does, where [pulse](/docs/tools/#pulse) is installed. pulse is the tool of the family that says what the machine is busy with, and lumen shows what it says where you work.

![lumen with three tabs. The first reads "cargo build" and shows 5.6 and 4.1G beside its name, the second shows 0.4 and 1.4G. A light at the right of the title bar is lit, and a note under it reads "busy: cpu at 13.2 of 16 cores, most from cargo build (5.6 cores)"](/shots/machine.png)

pulse is not public yet. Without it, nothing on this page is shown, and nothing else in lumen is missing.

## The light in the title bar

At the right of the title bar, next to the window's buttons, is a small light. It is **lit when pulse calls the machine busy**: the CPU or the graphics card is under load, a game runs, or memory is short. Otherwise it is faint.

Rest the mouse on it and it says why, in pulse's words, and how much of the CPU and the memory is in use.

**A click on it opens pulse** in a new tab: the full monitor, with every process.

## What a tab costs

A tab whose programs are at work says so beside its name:

| On the tab | Means |
| --- | --- |
| `5.6` | The work of 5.6 CPU cores. Shown from 0.2 on, and lit from 2 on |
| `4.1G` | 4.1 GB of memory. Shown from 1 GB on |

The numbers are for everything the tab's panes have started: a build with all its compilers, a server with all its workers. Rest the mouse on the tab and it names the heaviest job and the number of processes: `cargo build · 5.6 cores · 4.1 GB`.

So with a few tabs open you see which one the fans are for, without leaving the one you are in.

## What a command costs

With a shell that [marks its commands](/docs/commands/), the mark beside a command says more:

- **While it runs**: what its pane uses right now. `running for 8.2 s · 5.6 cores · 4.1 GB`.
- **When it has ended**: the most memory its pane was seen to use while it ran. `done in 48.2 s · up to 8.9 GB`.

pulse looks every three seconds, so "up to" is the most that was seen: a short peak between two looks is missed, and a command that was over within three seconds may have no number at all. Less than 100 MB is not mentioned.

## What it takes

- pulse on your `PATH`, or in `~/go/bin` or `~/.local/bin`, where lumen looks too.
- Nothing else. lumen starts one pulse for all its windows, tells it which shells to look at, and stops it when lumen ends. If pulse ends by itself, lumen starts it again.

## Settings

In the [config](/docs/configuration/#pulse):

```toml
[pulse]
enabled = true     # false: lumen does not start pulse
every = 3          # seconds between two looks, from 1 to 60
```

A shorter wait makes the numbers fresher and costs a little more of the machine.
