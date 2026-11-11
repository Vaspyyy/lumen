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

You need what [Install](/docs/install/) lists. Then:

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

## This site

The site is in the `site` folder of the same repository, and every page of these docs is a Markdown file in `site/src/content/docs`. Each page has a link at its end that opens it on GitHub for editing.

To see a change before you send it:

```sh
cd site
npm install
npm run dev
```

Release notes are Markdown files too, one per version, in `site/src/content/releases`.
