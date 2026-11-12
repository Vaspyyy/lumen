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
| Red | Failed: it ended with an exit status other than 0, and not because you stopped it with `Ctrl+C` |
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

These keys are lumen's in a pane whose shell has marked a prompt or a command. A full-screen program such as vim gets them itself. In a pane where nothing was ever marked, they go to the program as they always did.

## What a tab tells you

A tab you are not looking at shows a dot when something happened in it:

| Dot | Means |
| --- | --- |
| Grey | A command runs there |
| Green | A command there has ended |
| Red | A command there has failed |
| In lumen's accent color | A program there rang the bell |

Start a long build, go to another tab, and the dot tells you when to come back. It goes away when you look at the tab. If the whole window is in the background, the tab that is open in it gets its dot too when a command in it ends or fails, or the bell rings.

## When a program has something to tell

A program can ask for your attention: a build that is through, a long run that ended. Programs do that with a notification, which most terminals pass on to the desktop.

lumen passes it on **only when you are not looking**:

- In the tab you are looking at, nothing happens. What the program printed is in front of you.
- Any other tab gets a dot, and the desktop shows a notification with the program's words. A click on the notification goes to the pane it came from. The dot is in lumen's accent color, unless the tab already has a green or a red one: those stay.
- The same goes for the tab that is open in a window you have left for another program.

A program that never stops telling is not heard forever. Each pane gets a notification on the desktop at most once in three seconds, and five in all until you look at its tab again.

To use it in a script of your own, end it with a line like this one:

```sh
make all; printf '\033]777;notify;make;all is built\a'
```

To keep programs from posting on the desktop, set `notifications = false` in the [config](/docs/configuration/). The dot on the tab stays.

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
- **A prompt begins on a new line.** Where a command's output did not end with a line break, bash writes its prompt right behind it, and zsh with `PROMPT_SP` turned off writes its prompt over it. With lumen's lines read in, both ask for a line of their own first, so that the last line of the output stays, and stays with its command.
- **What runs before the prompt counts to the command.** A hook that prints before the prompt, as direnv does, has its lines copied with the command's output. In bash the time such hooks take is added to the command's time as well.
- **bash with `promptvars` turned off** can't say how a command ended. Its mark says that it ended, and neither turns red nor names an exit status.
- **A prompt that is written again stays one command.** A shell writes its prompt again under a list of completions, after `Ctrl+L`, and when the window changes its width. What is left of the earlier copy further up is not a command of its own, and it is not copied with what the command before it printed. bash and zsh tell lumen when a prompt is a new one, once they have read its lines. fish doesn't, so lumen goes by where its prompt stands: after the window was made narrower, a line of a fish prompt of several lines can stay behind as a prompt at which nothing was run.
- **A prompt at the right of the line** (zsh's `RPROMPT`) and the one before a further line of a command (`PS2`, in bash and zsh) are prompts to lumen, and no part of the command. That takes a shell on this machine that has read lumen's lines. Over SSH, the shell there is not told that it runs in lumen, and marks neither of them.
- **Over SSH** it works when the shell on the other machine marks its commands. A fish 4 there does. The same goes for a shell you start inside another. From the first prompt of the inner shell on, the marks are its own: the `ssh` command that led there is no longer followed, and its mark says how long it ran before that prompt.
- **Inside tmux or another multiplexer** there are no marks: it keeps them to itself.
- **In a full-screen program** nothing is marked. A shell you start inside one belongs to that program's screen.
- **A command that printed nothing** has nothing to copy, and lumen says so.
- Paths in a command's output are followed from the folder the command ran in, as far as your own shell knows it: for a shell inside another, or on another machine, that is the folder the outer shell is in. See [Files and links](/docs/files-and-links/#web-addresses-and-places-in-code).
- Where pulse is installed, a mark also says what its command costs. See [The machine](/docs/machine/#what-a-command-costs).
