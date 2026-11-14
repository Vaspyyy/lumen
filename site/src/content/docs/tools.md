---
title: The tools
description: The command line tools lumen is made for, and what lumen does with what they print.
---

lumen is the home of a family of command line tools. They share one idea with lumen: print less, and make what is printed useful.

The tools print ordinary terminal output and work in any terminal. lumen is a good terminal without them. Together, they do things neither does alone.

What each version of each tool brought is on this site too, beside lumen's own [release notes](/releases/): [glint](/releases/glint/), [crux](/releases/crux/), [recap](/releases/recap/), [seek](/releases/seek/), [pulse](/releases/pulse/), [splice](/releases/splice/), [peek](/releases/peek/) and [lens](/releases/lens/).

## glint

[glint](https://github.com/Vaspyyy/glint) is a modern `ls`: icons, git state, previews of what is in a file, thumbnails.

With lumen:

- **Names are files.** With `hyperlink = "copy"` in glint's config, a click on a name copies the file, `Ctrl+Click` opens it, and it can be dragged out. See [Files and links](/docs/files-and-links/).
- **Thumbnails are pictures.** `glint --thumbs` draws real thumbnails in lumen. In a terminal that can't show pictures, glint draws them out of colored blocks instead. See [Pictures](/docs/images/).

glint 0.11.1 or newer knows lumen. An older one can be told to send pictures with `--thumbs=kitty`.

```sh
go install github.com/Vaspyyy/glint@latest
```

## crux, seek and recap

- [crux](https://github.com/Vaspyyy/crux) runs a build, a test or a linter and prints only what matters: the verdict, the counts, and each failure with its place in the code.
- [seek](https://github.com/Vaspyyy/seek) is a code search that knows which function each hit is in.
- [recap](https://github.com/Vaspyyy/recap) is a git diff by function and type instead of by hunk.

All three print places in code as `path:line`, such as `src/view.rs:412`.

With lumen:

- **A place is a click away.** Hold `Ctrl` and click one, and the file opens in your editor at that line. See [Files and links](/docs/files-and-links/#web-addresses-and-places-in-code).
- **A run is one thing.** With a shell that marks its commands, the output of a `crux` run is copied with one click, and the tab says when a long run has ended. See [Commands](/docs/commands/).

## pulse

[pulse](https://github.com/Vaspyyy/pulse) says what the machine is busy with: how much of the CPU, the memory and the graphics card is in use, by what, and whether it is too busy for a measurement to mean anything.

With lumen:

- **The title bar has a light** that is lit while the machine is busy, and says why when the mouse rests on it. A click opens pulse.
- **A tab says what it costs**: the CPU cores and the memory of everything its panes have started.
- **A command's mark says what it costs**: while it runs, and the most memory it was seen with when it has ended.
- **What pulse names can be clicked.** The commands it suggests are links that put the command at your prompt, with `Enter` left to you. See [Files and links](/docs/files-and-links/#links-that-type-a-command).
- **A long `pulse run` tells you when it is over**, if you are in another tab by then. See [Commands](/docs/commands/#when-a-program-has-something-to-tell).

See [The machine](/docs/machine/).

```sh
go install github.com/Vaspyyy/pulse@latest
```

## What the tools don't do

Nothing in the tools asks whether it runs in lumen in order to work. glint's links are standard links that any modern terminal passes on, and its pictures use a protocol several terminals speak. lumen only understands more of what is already there.

That is a rule the project keeps: a tool that needs one terminal is a worse tool, and a terminal that needs its own tools is a worse terminal.
