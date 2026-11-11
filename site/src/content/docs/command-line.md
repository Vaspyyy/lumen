---
title: Command line
description: How to start lumen from a script, a launcher or another terminal.
---

```
lumen [options] [-e program [args...]]
```

## Options

| Option | Does |
| --- | --- |
| `-e`, `--command` *program* *[args...]* | Runs this program instead of your shell. Everything after it belongs to the program |
| `-d`, `--working-directory` *dir* | Starts in this folder |
| `--config-path` | Prints where the config file is read from, and ends |
| `-v`, `--version` | Prints the version, and ends |
| `-h`, `--help` | Prints a short help, and ends |

## Examples

```sh
lumen                               # a window with your shell
lumen -d ~/Projects/site            # start in a folder
lumen -e htop                       # run a program instead of the shell
lumen -d ~/notes -e nvim todo.md    # both
```

`-e` takes the rest of the line, so it comes last. In `lumen -e ls -d`, the `-d` is for `ls`.

A window started with `-e` closes when the program ends.

## As the default terminal

The install script adds a launcher entry, `lumen.desktop`, so lumen can be chosen as the terminal in your desktop's settings. Desktops and launchers that start a terminal with a command usually pass it after `-e`, which lumen takes.

## What programs find

lumen sets these for every program it starts:

| Variable | Value |
| --- | --- |
| `TERM` | `xterm-256color` |
| `COLORTERM` | `truecolor` |
| `TERM_PROGRAM` | `lumen` |
| `TERM_PROGRAM_VERSION` | lumen's version, such as `0.2.1` |

A script can tell that it runs in lumen by `TERM_PROGRAM`:

```sh
if [ "$TERM_PROGRAM" = lumen ]; then
    glint --thumbs
fi
```

More about what a program can rely on is in [For programs](/docs/for-programs/).
