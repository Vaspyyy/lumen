---
title: Opening lumen again
nav: Opening lumen again
description: Starting lumen while one runs opens a tab in the window you used last. A daemon started at login makes the first window just as quick.
---

## A tab, not a second lumen

When you start `lumen` while a lumen runs already, from the application menu, a launcher or another terminal, it doesn't start a second one. The lumen that runs opens a **new tab in the window you used last** and brings that window to the front. The new tab starts in the folder you started `lumen` in, with that environment.

That is quicker than any new window can be: the window is there, and so are the GPU, the fonts and the config. Measured on a desktop PC, from the start of `lumen -e` to the program running in the new tab: about **8 ms**. A lumen that starts from nothing takes about 200 ms to get that far.

```sh
lumen                         # a new tab in the window you used last
lumen -d ~/Projects/site      # a new tab in that folder
lumen -e htop                 # a new tab running htop
lumen --new-window            # a new window, of the lumen that runs
lumen --new-process           # a lumen of its own, as if none ran
```

With `-e`, the `lumen` command waits until the program in the tab ends, as it would with a window of its own. A script that runs `lumen -e vim notes.md` goes on when you close vim.

## Windows instead of tabs

`launch` in the [config](/docs/configuration/) says what a launch opens:

```toml
launch = "tab"       # a tab in the window you used last (the default)
launch = "window"    # a window of its own, of the lumen that runs
launch = "process"   # a lumen of its own every time, as lumen did before
```

A window of the lumen that runs opens at once too, it only skips the wait for the GPU, the fonts and the config. `--new-window` and `--new-process` on the command line say the same for one launch, whatever the config says. The launcher entry's *New Window* action uses `--new-window`.

## When a launch starts a lumen of its own

The lumen that runs leaves a launch to start on its own when:

- **It is another version.** After an update, the lumen that still runs from before keeps its windows, and a launch starts the new version beside it. Once the old one has ended, the new one takes over, and the next launch opens a tab there.
- **It runs on another display**, such as a desktop session of the same user on another seat.
- **`launch = "process"`** is set.

A launch that finds nothing to talk to, or no answer within two seconds, starts on its own too, so a lumen that hangs never keeps you from a terminal.

## The daemon: a quick first window

A launch is quick when a lumen runs. The first one of the day is not: it starts the GPU and loads the fonts and the config, about 200 ms before the first frame.

`lumen --daemon` is a lumen that runs without a window. It does that work ahead of time, and then waits. The first `lumen` of a session hands its launch to the daemon, which opens the window. Measured on the same PC, from the start of `lumen` to the window's first frame:

| | First frame |
| --- | ---: |
| A lumen that starts from nothing | about 220 ms |
| With the daemon | about **45 ms** |

When the daemon's last window closes, it doesn't end. It lets go of the display and waits for the next launch, with the GPU started again.

### Started at login

The daemon is started by systemd, as a service of your user:

- **The install script** ([Install](/docs/install/#any-other-distribution)) and **`scripts/install.sh`** install the service and turn it on, where systemd runs for your user. With `LUMEN_DAEMON=0` in front of the command they leave it out. A service you turned off stays off when you install an update.
- **The packages** install the service, but leave it off, as packages do. Turn it on with:

```sh
systemctl --user enable --now lumen
```

To turn it off again:

```sh
systemctl --user disable --now lumen
```

That ends the daemon, and with it the windows it has open, the one you typed this in too if it is one of them. What you started from its terminals and that outlives a terminal, such as a tmux server or a job started with `nohup`, keeps running: it is yours, not the service's.

Without systemd, start `lumen --daemon` from your desktop's or compositor's autostart. It needs no display when it starts: each launch tells it which one to open its window on.

### What it costs

While it waits, the daemon uses about 25 MB of memory of its own and 20 MB on the graphics card. The rest of what tools show for it, about 130 MB, is the graphics driver's libraries, which every program that draws with the GPU shares. It does next to no work: every five seconds it looks whether lumen was updated.

### After an update

A daemon from before an update makes way for the new version by itself: within a few seconds of the update, if it has no window open, it ends, and systemd starts the new program in its place. A daemon with windows open keeps them, and makes way a few seconds after you close the last one. Until then, a launch of the new version starts a lumen of its own.

## Focus

A launcher that starts lumen hands it a token that allows a window to take the focus. lumen passes it on to the lumen that runs, so the window with the new tab comes to the front, on Wayland too. A launch from a terminal has no token: on Wayland, the compositor may then only mark the window as wanting attention.
