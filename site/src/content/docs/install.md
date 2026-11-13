---
title: Install
description: Packages for Debian, Ubuntu, Fedora, openSUSE and Arch, one command for every other distribution, the app for Android, or build it yourself.
---

lumen runs on Linux, on Wayland and X11. Every release comes as a package for the big distributions and as one command for the others. They are built for 64-bit Intel and AMD machines (`x86_64`); on any other machine, [build lumen from source](#build-it-from-source).

Each way installs the program, the launcher entry and the icons, so lumen shows up in your desktop's application menu, a systemd service that keeps lumen waiting from login on, so that its first window opens at once, and a timer that [updates](#updates) lumen and its tools once a day. The install scripts turn both on where systemd runs for your user, the packages leave them off.

lumen is drawn by the graphics card, so it needs a working graphics driver, with Vulkan or OpenGL. Any Mesa driver of the last few years will do.

## Debian and Ubuntu

```sh
curl -fLO https://github.com/Vaspyyy/lumen/releases/latest/download/lumen-terminal_amd64.deb
sudo apt install ./lumen-terminal_amd64.deb
```

For Debian 12 and Ubuntu 22.04, for newer versions of both, and for the systems built on them, such as Linux Mint.

## Fedora and openSUSE

On Fedora:

```sh
curl -fLO https://github.com/Vaspyyy/lumen/releases/latest/download/lumen-terminal.x86_64.rpm
sudo dnf install ./lumen-terminal.x86_64.rpm
```

On openSUSE Tumbleweed, the same file:

```sh
curl -fLO https://github.com/Vaspyyy/lumen/releases/latest/download/lumen-terminal.x86_64.rpm
sudo zypper install --allow-unsigned-rpm ./lumen-terminal.x86_64.rpm
```

For the Fedora releases that still get updates. Enterprise systems such as RHEL 9 and the ones built from it are too old for the prebuilt program: there, [build lumen from source](#build-it-from-source).

## Arch

```sh
curl -fLO https://github.com/Vaspyyy/lumen/releases/latest/download/lumen-terminal-x86_64.pkg.tar.zst
sudo pacman -U lumen-terminal-x86_64.pkg.tar.zst
```

Also for the systems built on Arch, such as CachyOS, EndeavourOS and Manjaro.

The package is called `lumen-terminal`. The AUR has a package called `lumen`, which is a different program with a command of the same name, so the two can't be installed next to each other.

## Any other distribution

One command for every system with a C library as new as the one in Ubuntu 22.04 (glibc 2.35):

```sh
curl -fsSL https://vaspyyy.github.io/lumen/install.sh | sh
```

This needs no root. The script fetches the newest release, checks that it arrived whole against the checksum in the release, and puts the program into `~/.local/bin` and the launcher entry and the icons into `~/.local/share`. Where systemd runs for your user, it also installs the [daemon](#the-daemon)'s service and the [daily update](#updates)'s timer into `~/.config/systemd/user` and turns them on; `LUMEN_DAEMON=0` or `LUMEN_AUTO_UPDATE=0` in front of `sh` leaves one out. If you like to know what you run, [read it first](https://github.com/Vaspyyy/lumen/blob/main/packaging/install.sh): it is about 450 lines.

lumen needs a few libraries that a desktop has anyway:

- xkbcommon with its X11 part, and XCB. lumen can't start without them. When one is missing, the script names it and installs nothing.
- The Vulkan loader or EGL, and on Wayland the Wayland client library. lumen loads them when it opens its window. When one is missing, the script installs lumen and tells you.

Older systems, and the ones with another C library than glibc, such as Alpine, can't run the prebuilt program. The script says so and installs nothing. There, [build lumen from source](#build-it-from-source).

The script doesn't touch another program that is called `lumen` too: if one is in `~/.local/bin`, it stops, and `LUMEN_BIN_DIR` names another folder for this one.

If `~/.local/bin` is not on your `PATH`, the script says so. lumen still starts from the application menu.

## The Android app

[lumen on your phone](/docs/phone/) is an app of its own, for Android 11 and newer. It is not in the Play Store: every release has it as a file, to open on the phone.

[**lumen-android.apk**](https://github.com/Vaspyyy/lumen/releases/latest/download/lumen-android.apk)

Android asks once whether your browser may install apps. Allow it, and install. A newer version is installed over the old one the same way, and your paired PCs stay.

The file is signed with lumen's own key, and Android installs an update only if the same key signed it. To check a file before you install it, on a PC with the Android build tools:

```sh
apksigner verify --print-certs lumen-android.apk
```

The certificate's SHA-256 has to be:

```
e45200452aa3f2f6276debd70005230e01b7974e41931f66c232720903ff99ed
```

The file is also in `SHA256SUMS`, and GitHub attests where it was built, as for the packages: see [Check what you fetched](#check-what-you-fetched).

The app needs lumen 0.3.0 or newer on the PC.

## Start it

From your desktop's application menu, or from a terminal:

```sh
lumen
```

Started again while it runs, lumen opens a new tab in the window you used last. See [Opening lumen again](/docs/launching/).

## The daemon

`lumen --daemon` is a lumen without a window that waits from login on, so that the first window of a session opens in about 45 ms instead of 220. The install script and `scripts/install.sh` turn it on. With a package, turn it on yourself:

```sh
systemctl --user enable --now lumen
```

`systemctl --user disable --now lumen` turns it off again, and closes the windows it has open. [Opening lumen again](/docs/launching/#the-daemon-a-quick-first-window) says what it costs and how it behaves.

## Updates

lumen updates itself, and the tools made for it, once a day:

```sh
lumen update           # the same by hand, now
lumen update --check   # only say what is out
```

- **lumen** comes from its newest release: the archive is fetched, checked against the release's checksums, and installed into the folder lumen is in, with its launcher entry, icons and services. A lumen you built from source is replaced by the release once a release newer than it is out.
- **A package** (apt, dnf, zypper, pacman) can only be updated by its package manager, with root. lumen tells you once when a new version is out: install the new package the way you installed the first one. No distribution has lumen in its repositories yet, so your system's own updates don't bring it. `lumen update` doesn't run as root.
- **The tools** ([glint](https://github.com/Vaspyyy/glint), crux, recap, seek and pulse) are updated with `go install`, the way they were installed, into the folder their program file is in (a link to it, in `~/.local/bin` say, keeps working). That needs Go. A tool that isn't installed stays so, and so does another program of the same name: only what Go built from the tool's module is updated.

Nothing is ever replaced by an older version, so a build of your own that is newer than the newest release stays as it is. A build in a cargo `target` folder is never replaced.

The install script and `scripts/install.sh` turn the daily update on, where systemd runs for your user: a timer, `lumen-update.timer`, starts `lumen update` once a day at the lowest priority, so a game or a build doesn't notice it, and you get a notification of what changed (where `notify-send` is installed). When the network isn't there yet, it tries again half an hour later, twice at most. Something that fails the same way every time, a release that can't be installed here say, is told once in a notification, and the timer leaves that version alone; `lumen update` by hand tries it again. `journalctl --user -u lumen-update` shows what it did. It needs `curl` or `wget`. `LUMEN_AUTO_UPDATE=0` in front of the install command leaves it out. With a package, the timer is installed but off; turn it on to have the tools updated and to hear of new versions:

```sh
systemctl --user enable --now lumen-update.timer
```

`systemctl --user disable --now lumen-update.timer` turns it off again, and stays so through updates.

To change how the daemon or the daily update runs, use `systemctl --user edit lumen` or `systemctl --user edit lumen-update.timer`: the change goes into a file of its own, which installs and updates leave alone. A unit file in `~/.config/systemd/user` that you change in place, or write yourself, is yours from then on: the install scripts and `--uninstall` leave it as it is, and it gets no more changes from lumen.

A lumen that is running keeps running the old version until you close its windows. The [daemon](#the-daemon) then makes way for the new version by itself. The [release notes](/releases/) say what changed, and have a [feed](/rss.xml) to follow.

## Remove it

| Installed with | Remove with |
| --- | --- |
| `apt` | `sudo apt remove lumen-terminal` |
| `dnf` | `sudo dnf remove lumen-terminal` |
| `zypper` | `sudo zypper remove lumen-terminal` |
| `pacman` | `sudo pacman -R lumen-terminal` |
| The script | `curl -fsSL https://vaspyyy.github.io/lumen/install.sh \| sh -s -- --uninstall`. This stops the daemon and removes its service and the update's timer too |
| A build from source | see [Remove a build from source](#remove-a-build-from-source) |

If you turned the [daemon](#the-daemon) or the [daily update](#updates) on for a package, turn them off before you remove the package: `systemctl --user disable --now lumen lumen-update.timer`. The install script's `--uninstall` does that itself.

Your settings are in `~/.config/lumen`. Remove that folder too if you want nothing left.

## Check what you fetched

Every release has a file `SHA256SUMS` with the checksum of each of its files. In the folder you downloaded into:

```sh
curl -fLO https://github.com/Vaspyyy/lumen/releases/latest/download/SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
```

The packages are built by a public workflow on GitHub, from the source of the tag their release is made from, and GitHub signs a statement that says so. With the [GitHub CLI](https://cli.github.com) you can check it:

```sh
gh attestation verify lumen-terminal_amd64.deb --repo Vaspyyy/lumen
```

## Tools worth having

Two small programs make lumen more useful, and both are optional:

- **`wl-copy`** (package `wl-clipboard`) on Wayland, or **`xclip`** on X11. lumen uses it to put a file on the clipboard when you [click a file name](/docs/files-and-links/).
- **[glint](https://github.com/Vaspyyy/glint)**, version 0.11.1 or newer. It prints the listings whose names are files (since 0.11.0) and the thumbnails (since 0.11.1).

## Build it from source

On a machine the packages are not built for, or to work on lumen itself. It needs a Rust toolchain and a handful of system libraries, and takes a few minutes the first time.

### What you need

- **A recent Rust toolchain.** [rustup](https://rustup.rs) installs one.
- **The development files of a few libraries** that the window toolkit links against.

On Debian and Ubuntu, the libraries are:

```sh
sudo apt install libxkbcommon-x11-dev libwayland-dev libx11-xcb-dev \
    libfontconfig-dev libvulkan-dev libasound2-dev
```

That list is the one lumen's own tests are built with. Other distributions have the same libraries under their own names: xkbcommon (with its X11 part), Wayland, XCB, Fontconfig, the Vulkan loader and ALSA.

### Build and install

```sh
git clone https://github.com/Vaspyyy/lumen
cd lumen
scripts/install.sh
```

The script does two things:

1. It builds lumen and puts the program into Cargo's bin folder, `~/.cargo/bin`. rustup adds that folder to your `PATH`.
2. It adds the launcher entry and the icons to `~/.local/share`, so lumen shows up in your desktop's application menu.
3. Where systemd runs for your user, it installs the [daemon](#the-daemon)'s service and the [daily update](#updates)'s timer into `~/.config/systemd/user` and turns them on. `LUMEN_DAEMON=0` or `LUMEN_AUTO_UPDATE=0` in front of it leaves one out.

Once a release newer than your build is out, the [daily update](#updates) replaces it with that release. To keep building from source instead, get the newest source with `git pull` and run the script again, or turn the daily update off.

### Try it without installing

To only have a look, build and run it from the folder:

```sh
cargo run --release
```

Nothing is copied anywhere. The first build takes the longest.

### Remove a build from source

```sh
cargo uninstall lumen
rm ~/.local/share/applications/lumen.desktop
rm ~/.local/share/icons/hicolor/*/apps/lumen.png
systemctl --user disable --now lumen-update.timer
systemctl --user disable lumen
rm ~/.config/systemd/user/lumen.service ~/.config/systemd/user/lumen-update.*
systemctl --user daemon-reload
systemctl --user stop lumen
```

The last line ends the daemon and closes its windows, so it comes last.
