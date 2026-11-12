---
title: Install
description: Packages for Debian, Ubuntu, Fedora, openSUSE and Arch, one command for every other distribution, the app for Android, or build it yourself.
---

lumen runs on Linux, on Wayland and X11. Every release comes as a package for the big distributions and as one command for the others. They are built for 64-bit Intel and AMD machines (`x86_64`); on any other machine, [build lumen from source](#build-it-from-source).

Each way installs the program, the launcher entry and the icons, so lumen shows up in your desktop's application menu, and a systemd service that keeps lumen waiting from login on, so that its first window opens at once. The packages leave that service off: see [The daemon](#the-daemon).

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

This needs no root. The script fetches the newest release, checks that it arrived whole against the checksum in the release, and puts the program into `~/.local/bin` and the launcher entry and the icons into `~/.local/share`. It also installs the [daemon](#the-daemon)'s service into `~/.config/systemd/user` and turns it on; `LUMEN_DAEMON=0` in front of `sh` leaves it out. If you like to know what you run, [read it first](https://github.com/Vaspyyy/lumen/blob/main/packaging/install.sh): it is about 300 lines.

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

`systemctl --user disable --now lumen` turns it off again. [Opening lumen again](/docs/launching/#the-daemon-a-quick-first-window) says what it costs and how it behaves.

## Update

lumen doesn't update itself, and no distribution has it in its repositories yet, so your system's updates don't bring new versions. To update, run the commands you installed it with again: they always fetch the newest release.

A lumen that is running keeps running the old version until you close its windows. The daemon then makes way for the new version by itself. The [release notes](/releases/) say what changed, and have a [feed](/rss.xml) to follow.

## Remove it

| Installed with | Remove with |
| --- | --- |
| `apt` | `sudo apt remove lumen-terminal` |
| `dnf` | `sudo dnf remove lumen-terminal` |
| `zypper` | `sudo zypper remove lumen-terminal` |
| `pacman` | `sudo pacman -R lumen-terminal` |
| The script | `curl -fsSL https://vaspyyy.github.io/lumen/install.sh \| sh -s -- --uninstall`. This stops the daemon and removes its service too |
| A build from source | see [Remove a build from source](#remove-a-build-from-source) |

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
3. It installs the [daemon](#the-daemon)'s service into `~/.config/systemd/user` and turns it on, unless `LUMEN_DAEMON=0` is in front of it.

To update, get the newest source with `git pull` and run the script again.

### Try it without installing

To only have a look, build and run it from the folder:

```sh
cargo run --release
```

Nothing is copied anywhere. The first build takes the longest.

### Remove a build from source

```sh
systemctl --user disable --now lumen
rm ~/.config/systemd/user/lumen.service
cargo uninstall lumen
rm ~/.local/share/applications/lumen.desktop
rm ~/.local/share/icons/hicolor/*/apps/lumen.png
```
