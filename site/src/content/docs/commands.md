---
title: Commands
description: lumen knows where each command starts and ends. Jump between them, copy what one printed, and see which ones failed.
---

To most terminals, what scrolls past is one long stream of text. lumen knows where each command starts, where its output ends and how it ended. So a command and what it printed are one thing you can go to and copy.

![Three commands in lumen with a mark beside each prompt: faint for one that went well, red for one that failed, lit for one that still runs. A note above the first reads "done in 1 ms, click copies the output", and a second tab has a green dot](/shots/commands.png)

## The mark beside a command

Every command you have run has a small mark in the margin, beside its prompt:

| The mark is | The command |
| --- | --- |
| Lit, in lumen's accent color | Still runs |
| Red | Failed: it ended with an exit status other than 0 |
| Faint | Ended well, or was stopped with `Ctrl+C` |

Move the mouse onto a mark and lumen says how it went: `done in 2.4 s`, `exit 1 after 12 ms`, `running for 3 min 12 s`.

**Click the mark** and what the command printed is on the clipboard. lumen selects it, so you see what was copied, and says so at the bottom of the window: `Copied the output of cargo test: 48 lines`.

A prompt where nothing was run has no mark.

## Keys

| Key | Does |
| --- | --- |
| `Ctrl+Shift+Up` | Go to the command before: its prompt becomes the first line |
| `Ctrl+Shift+Down` | Go to the next command. After the last one, back to the bottom |
| `Ctrl+Shift+O` | Copy the output of the last command, also of one that still runs. When you have scrolled back, of the command at the top of the view |

A full-screen program such as vim gets these keys itself. They are lumen's only at a shell.

## What a tab tells you

A tab you are not looking at shows a dot when something happened in it:

| Dot | Means |
| --- | --- |
| Grey | A command runs there |
| Green | A command there has ended |
| Red | A command there has failed |
| In lumen's accent color | A program there rang the bell |

Start a long build, go to another tab, and the dot tells you when to come back. It goes away when you look at the tab. If the whole window is in the background, the tab that is open in it gets a dot too.

## Setting up your shell

The shell has to tell the terminal where its commands are. That is a standard other terminals read too (OSC 133), so nothing here is for lumen alone.

**fish** does it by itself from version 4.0 on. There is nothing to set up.

**bash**: add this as the last line of `~/.bashrc`:

```sh
eval "$(lumen shell-integration bash)"
```

**zsh**: add this as the last line of `~/.zshrc`:

```sh
eval "$(lumen shell-integration zsh)"
```

**fish older than 4.0**: add this to `~/.config/fish/config.fish`:

```sh
lumen shell-integration fish | source
```

Open a new tab, and commands have marks from then on. `lumen shell-integration bash` prints the lines it adds, if you would like to read them first. They put the marks around your prompt, and leave the prompt itself as it is.

If the same startup file is used on machines without lumen, ask for it only where it is there:

```sh
command -v lumen >/dev/null && eval "$(lumen shell-integration bash)"
```

Without marks, lumen is the terminal it was before: nothing is missing but what this page describes.

## Good to know

- **The marks need room.** They stand in the space around a pane's text, which is [`padding`](/docs/configuration/) in the config. With less than 4 pixels of it they are not drawn. The keys work all the same.
- **Over SSH** it works when the shell on the other machine marks its commands. A fish 4 there does.
- **Inside tmux or another multiplexer** there are no marks: it keeps them to itself.
- **In a full-screen program** nothing is marked. A shell you start inside one belongs to that program's screen.
- **A command that printed nothing** has nothing to copy, and lumen says so.
- Paths in a command's output are followed from the folder the command ran in. See [Files and links](/docs/files-and-links/#web-addresses-and-places-in-code).
