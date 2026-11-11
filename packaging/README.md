# Packaging

What a release of lumen offers, and how it is made.

| File of a release | For | Made by |
| --- | --- | --- |
| `lumen-x86_64-linux.tar.gz` | any distribution, with `install.sh` | `package.sh` |
| `lumen-terminal_amd64.deb` | Debian, Ubuntu | `package.sh` |
| `lumen-terminal.x86_64.rpm` | Fedora, openSUSE | `package.sh`, from `rpm/lumen-terminal.spec` |
| `lumen-terminal-x86_64.pkg.tar.zst` | Arch | `arch/build.sh`, from `arch/PKGBUILD` |
| `SHA256SUMS` | checking the others | the release workflow |

The names carry no version, so that `releases/latest/download/<name>` always leads to the newest one.

## A release

1. Set the version in `Cargo.toml` (and let Cargo update `Cargo.lock`).
2. Write its note: `site/src/content/releases/<version>.md`.
3. Push that to `main`, and wait until its checks have passed: the release workflow builds and tries the packages, and runs no tests of its own.
4. Push a tag: `git tag v<version> && git push origin v<version>`.

The website shows the new note as soon as it is on `main`. The files follow when the workflow is through, about half an hour later; until then the install commands fetch the release before.

The workflow in `.github/workflows/release.yml` does the rest. It builds lumen on Ubuntu 22.04, because a program runs on every system newer than the one it was built on and on none that is older. It makes the files above, tries each of them on a clean system with `verify.sh`, and publishes them only when every try went well. Started by hand, from the Actions page, it does everything except publishing.

## On your own machine

With Docker, the same steps:

```sh
# build, in the old system the workflow uses
docker run --rm -v "$PWD:/lumen" -w /lumen -e CARGO_TARGET_DIR=/build ubuntu:22.04 sh -c '
    apt-get update &&
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        ca-certificates curl build-essential pkg-config dpkg-dev rpm \
        libxkbcommon-x11-dev libwayland-dev libx11-xcb-dev libfontconfig-dev libvulkan-dev libasound2-dev &&
    curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal &&
    . "$HOME/.cargo/env" &&
    cargo build --release --locked &&
    LUMEN_BIN=/build/release/lumen packaging/package.sh &&
    chown -R "$(stat -c %u:%g .)" dist'

# the Arch package
docker run --rm -v "$PWD:/lumen" archlinux:base-devel sh -c '
    sh /lumen/packaging/arch/build.sh &&
    chown -R "$(stat -c %u:%g /lumen)" /lumen/dist'

# try one: install it, open a window nobody sees, remove it
docker run --rm -v "$PWD:/lumen:ro" debian:12 sh /lumen/packaging/verify.sh deb
```

`verify.sh` says at its top which systems it knows. The results are in `dist/`. The build happens inside the container, in `/build`, so your own `target` folder stays as it is.

## What the packages depend on

lumen is linked against xkbcommon, its X11 part, and XCB: the tools that build the packages find those themselves. Vulkan, EGL and the Wayland client library are loaded when lumen starts, so no tool sees them, and each package names them by hand. `fc-match`, from fontconfig, tells lumen which fonts the system has.

The Arch package is called `lumen-terminal` and conflicts with `lumen`, `lumen-bin` and `lumen-git` from the AUR: a different program, whose command has the same name.
