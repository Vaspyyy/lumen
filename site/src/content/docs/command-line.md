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

## Phones and kept terminals

These need no window. They run in any terminal, and over SSH.

| Command | Does |
| --- | --- |
| `lumen pair` | Pairs a phone: shows a code for the lumen app to scan, waits ten minutes for the phone, and asks before it lets it in |
| `lumen phones` | Lists the paired phones with the fingerprints of their keys |
| `lumen unpair` *name* | A paired phone can no longer log in |
| `lumen attach --session` *name* | Shows a terminal the PC keeps running, and makes it if there is none of that name. It is then shown here, and no longer where it was shown before |
| `lumen sessions` | Lists the terminals the PC keeps running. With `--json`, as JSON |
| `lumen sessions kill` *name* | Ends one. With `--all` in place of the name, all of them |

`lumen pair` takes:

| Option | Does |
| --- | --- |
| `--label` *name* | What the app calls this PC. Without it, the PC's host name |
| `--address` *address* | An address the phone can reach this PC at, tried before the ones lumen finds itself. Can be given more than once |
| `--port` *number* | The SSH server's port, if lumen can't tell |
| `--host-key` *file* | A public key the SSH server proves itself with, if they are not in `/etc/ssh` |
| `--authorized-keys` *file* | The file to add the phone's key to, if it is not `~/.ssh/authorized_keys`. `lumen phones` and `lumen unpair` take it too |
| `-y`, `--yes` | Lets the first phone that comes with the code in, without asking, also when its key takes the place of one it had before. For scripts |

`lumen attach` takes `--cwd` *folder* and `--env` *NAME=value* for a terminal it makes.

[lumen on your phone](/docs/phone/) is where to start, [How the phone logs in](/docs/phone-security/) says what pairing changes on the PC, and [Terminals that keep running](/docs/kept-terminals/) is about `attach` and `sessions`.

## Shells

| Command | Does |
| --- | --- |
| `lumen shell-integration bash` | Prints the lines that make bash mark its commands |
| `lumen shell-integration zsh` | The same for zsh |
| `lumen shell-integration fish` | The same for fish older than 4.0. Newer ones need none |

They are printed, and written nowhere: you add one line to your shell's startup file that reads them in. [Commands](/docs/commands/#setting-up-your-shell) has the line for each shell.

## As the default terminal

The install script adds a launcher entry, `lumen.desktop`, so lumen can be chosen as the terminal in your desktop's settings. Desktops and launchers that start a terminal with a command usually pass it after `-e`, which lumen takes.

## What programs find

lumen sets these for every program it starts:

| Variable | Value |
| --- | --- |
| `TERM` | `xterm-256color` |
| `COLORTERM` | `truecolor` |
| `TERM_PROGRAM` | `lumen` |
| `TERM_PROGRAM_VERSION` | lumen's version, such as `0.3.0` |

A script can tell that it runs in lumen by `TERM_PROGRAM`:

```sh
if [ "$TERM_PROGRAM" = lumen ]; then
    glint --thumbs
fi
```

More about what a program can rely on is in [For programs](/docs/for-programs/).
