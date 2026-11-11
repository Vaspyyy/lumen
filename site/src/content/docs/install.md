---
title: Install
description: lumen is built from source. It takes three commands and a few minutes.
---

lumen is not packaged for any distribution yet, so you build it yourself. That needs a Rust toolchain and a handful of system libraries, and takes a few minutes the first time.

## What you need

- **Linux**, with Wayland or X11.
- **A graphics driver with Vulkan.** lumen is drawn by the graphics card. Any Mesa driver of the last few years has it.
- **A recent Rust toolchain.** [rustup](https://rustup.rs) installs one.
- **The development files of a few libraries** that the window toolkit links against.

On Debian and Ubuntu, the libraries are:

```sh
sudo apt install libxkbcommon-x11-dev libwayland-dev libx11-xcb-dev \
    libfontconfig-dev libvulkan-dev libasound2-dev
```

That list is the one lumen's own tests are built with. Other distributions have the same libraries under their own names: xkbcommon (with its X11 part), Wayland, XCB, Fontconfig, the Vulkan loader and ALSA.

## Build and install

```sh
git clone https://github.com/Vaspyyy/lumen
cd lumen
scripts/install.sh
```

The script does two things:

1. It builds lumen and puts the program into Cargo's bin folder, `~/.cargo/bin`. rustup adds that folder to your `PATH`.
2. It adds the launcher entry and the icons to `~/.local/share`, so lumen shows up in your desktop's application menu.

Then start it from the menu, or from a terminal:

```sh
lumen
```

## Try it without installing

To only have a look, build and run it from the folder:

```sh
cargo run --release
```

Nothing is copied anywhere. The first build takes the longest.

## Update

Get the newest source and run the script again:

```sh
cd lumen
git pull
scripts/install.sh
```

A lumen that is running keeps running the old version until you close it. The [release notes](/releases/) say what changed.

## Tools worth having

Two small programs make lumen more useful, and both are optional:

- **`wl-copy`** (package `wl-clipboard`) on Wayland, or **`xclip`** on X11. lumen uses it to put a file on the clipboard when you [click a file name](/docs/files-and-links/).
- **[glint](https://github.com/Vaspyyy/glint)**, version 0.11.1 or newer. It prints the listings whose names are files, and the thumbnails.

## Remove it

```sh
cargo uninstall lumen
rm ~/.local/share/applications/lumen.desktop
rm ~/.local/share/icons/hicolor/*/apps/lumen.png
```

Your settings are in `~/.config/lumen`. Remove that folder too if you want nothing left.
