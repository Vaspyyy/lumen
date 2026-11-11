#!/bin/sh
# Builds lumen and installs it for this user: the program into Cargo's bin
# folder, the launcher entry and the icons into ~/.local/share.
set -e
CDPATH='' cd -- "$(dirname -- "$0")/.."
cargo install --path . --locked
data=${XDG_DATA_HOME:-$HOME/.local/share}
install -Dm644 packaging/lumen.desktop "$data/applications/lumen.desktop"
for size in 32 48 64 128 256 512; do
    install -Dm644 "assets/icon/lumen-$size.png" "$data/icons/hicolor/${size}x${size}/apps/lumen.png"
done
command -v update-desktop-database >/dev/null && update-desktop-database "$data/applications" || true
# A list of the icons, where a desktop keeps one, counts for as long as the
# folder is no newer than it: with a new date on the folder the icons are
# looked up afresh. (Making such a list here would hide the icons that other
# programs add later.)
touch "$data/icons/hicolor"
echo "installed lumen $(lumen --version | cut -d' ' -f2)"
