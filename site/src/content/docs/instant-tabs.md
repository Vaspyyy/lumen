---
title: Instant tabs
nav: Instant tabs
description: With prewarm on, a new tab shows a finished prompt the moment it opens.
---

```toml
prewarm = true
```

## The wait this removes

When you open a tab, a terminal starts your shell, and the shell reads its startup files before it shows a prompt. With a plain shell that takes a few hundredths of a second. With plugins, a prompt theme and a greeter it takes a quarter of a second or more, every time, and for that time the new tab is empty.

With `prewarm` on, lumen starts the shell of the *next* tab ahead of time and keeps it waiting in the background. When you ask for a tab, the shell is already at its prompt.

Measured with fish on a desktop PC, from the key press to a finished prompt on screen:

| | |
| --- | --- |
| A shell started when you ask | about 230 ms |
| With `prewarm` | about 30 ms |

A split gets its shell the same way.

## What it does, exactly

- lumen keeps **one** shell started ahead of time, per window. When you take it, the next one is started.
- It is started **in the folder of the pane you are in**, since that is where a new tab opens.
- When you change folders, the waiting shell is in the wrong place. lumen notices and replaces it. A tab opened right after a `cd` may still start its shell on the spot, as it would without `prewarm`.
- The waiting shell is an ordinary shell that nobody is looking at. It costs what one more idle shell costs: some memory (17 MB with fish, when measured), and no processor time.

## Why it is off by default

Your shell's startup files run **before you ask for the tab**: once when lumen starts, and again each time the waiting shell is used up or replaced.

For most setups that makes no difference. A greeter that prints system information is fine, it is simply already there when the tab opens.

It matters when a startup file does something that should happen only when you are there:

- it **attaches to tmux** or starts another program that takes over the terminal
- it **asks a question** and waits for an answer
- it **counts or logs** each shell that is started

If your startup files do none of that, turn it on.

## Speeding up the shell itself

`prewarm` hides a slow shell. It does not make it faster, and a shell that is slow to start is slow in other places too. To see what yours spends its time on:

```sh
fish --profile-startup /tmp/startup.log -c exit
sort -nk2 /tmp/startup.log | tail
```

That is for fish. zsh has `zprof` for the same job. A usual cause is a tool that sets up its completions on every start. Completions can almost always be loaded only when they are first needed.
