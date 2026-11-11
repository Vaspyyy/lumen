---
title: What lumen is
nav: Introduction
description: A terminal for Linux that knows what your tools print, and lets you use it.
---

lumen is a terminal emulator for Linux. It is drawn by the graphics card, has tabs and splits built in, and runs on Wayland and X11.

![lumen with two tabs and three panes: a file listing, a git log, and the results of a test run and a code search](/shots/panes.png)

What sets it apart is one idea: **the screen is not only text**. When a tool prints the name of a file, a picture or a link, most terminals draw the characters and stop there. lumen knows what they are.

- **A file name is the file.** In a listing, a click on a name copies the file to the clipboard, `Ctrl+Click` opens it, and you can drag it out of the window into another program. See [Files and links](/docs/files-and-links/).
- **Pictures sit between the lines.** What a program sends with the kitty graphics protocol is drawn in place and scrolls with the text. See [Pictures](/docs/images/).
- **A new tab is there at once.** With `prewarm` on, the next shell is started ahead of time. See [Instant tabs](/docs/instant-tabs/).

Around that is a terminal you can work in all day: [tabs and splits](/docs/tabs-and-splits/), [search](/docs/search/) through what scrolled past, a [config file](/docs/configuration/) that is taken on when you save it, and a cursor that glows.

## Where to start

| If you want to | Read |
| --- | --- |
| Get lumen onto your machine | [Install](/docs/install/) |
| Learn the handful of keys that matter | [First steps](/docs/first-steps/) |
| Change the font, the colors, the window | [Configuration](/docs/configuration/) |
| Know what a program can rely on in lumen | [For programs](/docs/for-programs/) |
| See what is planned | [Roadmap](/docs/roadmap/) |

## Early days

lumen is new, and it says so. It is a terminal you can use every day, and the first parts that make it different are in. Some things other terminals have are not there yet: key bindings can't be changed, there are no ligatures settings, and lumen is not packaged for any distribution, so it is built from source. The [roadmap](/docs/roadmap/) says what comes next, and the [release notes](/releases/) say what each version brought.

## The tools it is made for

lumen is the home of a family of command line tools: [glint](https://github.com/Vaspyyy/glint) (a modern `ls`), [crux](https://github.com/Vaspyyy/crux) (builds and tests, reduced to the verdict), [seek](https://github.com/Vaspyyy/seek) (a code search that knows functions) and [recap](https://github.com/Vaspyyy/recap) (a git diff by function).

Those tools print ordinary terminal output that works in any terminal. lumen understands more of it than other terminals do. Nothing in them depends on lumen, and lumen is a good terminal without them. See [The tools](/docs/tools/).
