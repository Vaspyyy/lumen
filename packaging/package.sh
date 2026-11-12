#!/bin/sh
# Makes what a release offers out of a built lumen: the archive with its
# install script, the Debian package and the RPM. The Arch package is made
# from the archive, by packaging/arch/build.sh.
#
#   cargo build --release --locked
#   packaging/package.sh
#
# Wants Debian or Ubuntu, with dpkg-dev and rpm. Build the program on an old
# system: it runs on every system newer than the one it was built on, and on
# none that is older. LUMEN_BIN names another program than
# target/release/lumen, DIST another folder than dist/ for the results.

set -eu
CDPATH='' cd -- "$(dirname -- "$0")/.."

fail() {
    echo "package.sh: $*" >&2
    exit 1
}

bin=${LUMEN_BIN:-target/release/lumen}
[ -x "$bin" ] || fail "no program at $bin: build it first"
bin=$(realpath "$bin")

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
built=$("$bin" --version)
[ "$built" = "lumen $version" ] || fail "$bin says \"$built\", Cargo.toml says $version"
[ -f "site/src/content/releases/$version.md" ] || fail "$version has no release note in site/src/content/releases"

machine=$(uname -m)
sizes="32 48 64 128 256 512"
mkdir -p "${DIST:-dist}"
dist=$(realpath "${DIST:-dist}")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# The files a system's package holds, laid out under $1.
tree() {
    install -Dm755 "$bin" "$1/usr/bin/lumen"
    install -Dm644 packaging/lumen.desktop "$1/usr/share/applications/lumen.desktop"
    install -Dm644 packaging/lumen.service "$1/usr/lib/systemd/user/lumen.service"
    for size in $sizes; do
        install -Dm644 "assets/icon/lumen-$size.png" "$1/usr/share/icons/hicolor/${size}x${size}/apps/lumen.png"
    done
    install -Dm644 README.md "$1/usr/share/doc/lumen-terminal/README.md"
}

# ---- the archive: for install.sh, and for the Arch package ----

name=lumen-$version-$machine-linux
mkdir -p "$work/$name/icons"
install -m755 "$bin" "$work/$name/lumen"
install -m755 packaging/install.sh "$work/$name/install.sh"
install -m644 packaging/lumen.desktop packaging/lumen.service LICENSE README.md "$work/$name/"
for size in $sizes; do
    install -m644 "assets/icon/lumen-$size.png" "$work/$name/icons/"
done
tar -C "$work" --sort=name --owner=0 --group=0 --numeric-owner -czf "$dist/lumen-$machine-linux.tar.gz" "$name"

# ---- Debian and Ubuntu ----

deb=$work/deb
tree "$deb"
cat >"$deb/usr/share/doc/lumen-terminal/copyright" <<'EOF'
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: lumen
Source: https://github.com/Vaspyyy/lumen

Files: *
Copyright: the authors of lumen
License: GPL-3+
 lumen is free software: you can redistribute it and/or modify it under the
 terms of the GNU General Public License as published by the Free Software
 Foundation, either version 3 of the License, or (at your option) any later
 version.
 .
 lumen is distributed in the hope that it will be useful, but WITHOUT ANY
 WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
 details.
 .
 On Debian systems, the complete text of the GNU General Public License
 version 3 can be found in "/usr/share/common-licenses/GPL-3".
EOF
size=$(du -sk "$deb" | cut -f1)

# the libraries the program is linked against, as the packages that hold them
mkdir -p "$work/shlibs/debian"
printf 'Source: lumen-terminal\n\nPackage: lumen-terminal\nArchitecture: any\n' >"$work/shlibs/debian/control"
linked=$(cd "$work/shlibs" && dpkg-shlibdeps -O "$deb/usr/bin/lumen" 2>"$work/shlibs.log" | sed -n 's/^shlibs:Depends=//p') || true
if [ -z "$linked" ]; then
    cat "$work/shlibs.log" >&2
    fail "dpkg-shlibdeps found no libraries"
fi

mkdir "$deb/DEBIAN"
# lumen loads Vulkan, EGL and Wayland when it starts: no tool finds those in the
# program. It asks fontconfig's fc-match which fonts the system has.
cat >"$deb/DEBIAN/control" <<EOF
Package: lumen-terminal
Version: $version-1
Architecture: $(dpkg --print-architecture)
Maintainer: Vaspyyy <Vaspyyy@users.noreply.github.com>
Installed-Size: $size
Depends: $linked, libvulkan1, libegl1, libwayland-client0, libwayland-egl1, fontconfig
Recommends: wl-clipboard | xclip
Section: x11
Priority: optional
Homepage: https://vaspyyy.github.io/lumen/
Description: terminal emulator that understands what your tools print
 lumen is a GPU-rendered terminal for Linux, on Wayland and X11, with tabs
 and splits built in. A file name in a listing is the file: a click copies
 it, Ctrl+Click opens it. Pictures a program sends are drawn in place.
EOF
(cd "$deb" && find usr -type f -exec md5sum {} + >DEBIAN/md5sums)
# xz, not Ubuntu's zstd: older Debian can't unpack that
dpkg-deb --root-owner-group -Zxz --build "$deb" "$dist/lumen-terminal_$(dpkg --print-architecture).deb" >/dev/null

# ---- Fedora, openSUSE and the others with RPM ----

rpm=$work/rpmtree
tree "$rpm"
install -Dm644 LICENSE "$rpm/usr/share/licenses/lumen-terminal/LICENSE"
rpmbuild -bb packaging/rpm/lumen-terminal.spec --quiet --target "$machine" \
    --define "_topdir $work/rpm" \
    --define "lumen_version $version" \
    --define "lumen_tree $rpm"
cp "$work"/rpm/RPMS/"$machine"/lumen-terminal-*.rpm "$dist/lumen-terminal.$machine.rpm"

ls -l "$dist"
