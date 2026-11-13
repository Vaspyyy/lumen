---
title: Contributing
description: How to build lumen, run its tests, and change this site.
---

lumen is free software under the GPL-3.0-or-later license, and its source is on [GitHub](https://github.com/Vaspyyy/lumen).

## Tell us what is wrong

The most useful thing is a good bug report. [Open an issue](https://github.com/Vaspyyy/lumen/issues) and say:

- what you did, what you expected, and what happened instead
- the version (`lumen --version`)
- your desktop, and whether it is Wayland or X11

If a program draws wrongly in lumen, say which program, and how to get it into that state.

## Build it

You need the Rust toolchain and the libraries that [Install](/docs/install/#build-it-from-source) lists for a build from source. Then:

```sh
git clone https://github.com/Vaspyyy/lumen
cd lumen
cargo run
```

`cargo run` builds a version for development. It is quicker to build and runs a little slower than the one `scripts/install.sh` makes.

## Before you send a change

Three commands have to pass. They are the ones that run on every push:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The tests cover the parts that can be tested without a window: what a key sends, what a mouse report looks like, how the config file is read, how picture commands are taken out of a program's output, where a link leads.

## How the code reads

A few habits, so that a change fits in:

- **Comments say why**, in plain sentences. What the code does is in the code.
- **Names are words**, not abbreviations.
- **A mistake is reported**, not ignored. A config file with a typo in it is an error with a message, not a silent default.
- **Nothing in lumen works for one tool only.** lumen reads standard escape sequences. Its one addition, the link that copies a file, is open to any program. If a tool needs something new, it should be something any terminal could add.

[How it works](/docs/how-it-works/) says what is where.

## Measure lag

When typing or the cursor feels slow, measure before you change anything, and again after. A lumen built with `--features lag` records where the time goes between a key and the frame that shows it, and `scripts/lag.py` turns the record into a report.

The quickest way runs on a display nobody sees, so it leaves your desktop alone and works over SSH. It needs [gamescope](https://github.com/ValveSoftware/gamescope):

```sh
scripts/lag.sh
```

It builds lumen with the recorder, starts it in a headless gamescope at 165 Hz, and lets it type a fixed scenario into your shell: about 15 seconds of fast typing, Ctrl+Backspace, key repeat and a held Backspace. It never presses Enter, so nothing is run. Then it prints the report. `LAG_HZ=60 scripts/lag.sh` measures at another refresh rate.

To record your own typing in a real window:

```sh
cargo build --release --features lag
LUMEN_LAG=~/lag.jsonl target/release/lumen
scripts/lag.py ~/lag.jsonl
```

Type as you normally do, then close the window. The record keeps which keys were pressed (any key that types text is just `char`, so what you typed isn't in it), how many bytes went to the program and came back, where the cursor was in each frame, and GPUI's own journal of input, draws and presents. With `LUMEN_LAG_DRIVE=1` the window types the scripted scenario instead, and quits after it.

To compare two builds, record each one and give both records to the report: `scripts/lag.py before.jsonl after.jsonl`.

The report has three parts:

- **From key to frame**: from lumen getting a key to the frame with the program's answer being handed to the compositor, split into the shell answering, the answer waiting for a frame, drawing and handing over. The compositor and the display add about one refresh on top.
- **Cursor glides**: how many frames the cursor's 70 ms glide gets, and how far apart they are. On a 165 Hz display a smooth glide gets about 12, one every 6 ms.
- **Frames**: how long a frame takes to draw, and the main thread's longest pauses with the line of code they were in.

Before you trust a number:

- Measure on an idle machine. `pulse load` says whether it is, and `scripts/lag.sh` won't measure on a busy one.
- GPUI holds frames back: to 30 a second for a window without the focus, to 60 when the computer runs hot. The report says when frames came slower than the display's rate.
- gamescope is not your desktop's compositor, and the scripted keys don't go through it. Use the hidden run to compare builds, and confirm a fix in a real window.

## This site

The site is in the `site` folder of the same repository, and every page of these docs is a Markdown file in `site/src/content/docs`. Each page has a link at its end that opens it on GitHub for editing.

To see a change before you send it:

```sh
cd site
npm install
npm run dev
```

Release notes are Markdown files too, one per version, in `site/src/content/releases`.

## How a release is made

A release is a tag. When a tag such as `v0.2.2` is pushed, a workflow builds lumen on an old system (so that it runs on every newer one), wraps it up as an archive, a Debian package, an RPM and an Arch package, and tries each of them on a clean system: it installs it, opens a lumen window on a display nobody sees, and removes it again. The same run builds the Android app, and a job of its own, the only one that can read the signing key, signs it and checks the signature. Only when all of that went well are the files published. The usual checks (format, lint, tests) are not part of it: they run when a change is pushed, so tag a commit that has passed them.

The version in `Cargo.toml` has to be the one the tag names, `versionName` in `android/app/build.gradle.kts` has to say the same, and the version needs its release note. The scripts are in `packaging`, and `packaging/README.md` says how to run them on your own machine.
