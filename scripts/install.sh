#!/bin/sh
# Builds lumen and installs it for this user: the program into Cargo's bin
# folder, the launcher entry and the icons into ~/.local/share, the systemd
# user service that keeps lumen waiting from login on (LUMEN_DAEMON=0 leaves it
# out) and the timer that updates lumen and its tools once a day
# (LUMEN_AUTO_UPDATE=0 leaves it out).
set -e
CDPATH='' cd -- "$(dirname -- "$0")/.."
cargo install --path . --locked
program=${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}/bin/lumen
data=${XDG_DATA_HOME:-$HOME/.local/share}
install -Dm644 packaging/lumen.desktop "$data/applications/lumen.desktop"
# The daemon's service (LUMEN_DAEMON=0 leaves it out) and the daily update's timer
# (LUMEN_AUTO_UPDATE=0), with this program's path. Without systemd for this user
# (over SSH, say) there is nothing to turn them on with.
units=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user
# quoted as systemd reads a path: \ and " escaped, % and $ doubled
quoted=$(printf '%s' "$program" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' -e 's/%/%%/g' -e 's/\$/$$/g')
# Whether $1 is a service or timer lumen wrote and nobody changed since: each ends
# with a checksum of the lines above it, as packaging/install.sh writes them. One
# changed or written by hand stays as it is (`systemctl --user edit` keeps changes
# in a drop-in, which an install leaves alone).
seal='# Written by lumen'"'"'s installer, which replaces it while this checksum fits:'
unit_sum() {
    grep -v -F -e "$seal" "$1" | sha256sum 2>/dev/null | cut -d ' ' -f 1
}
ours() {
    sealed=$(grep -F -e "$seal" "$1" 2>/dev/null | tail -n 1)
    sealed=${sealed#"$seal "}
    sum=$(unit_sum "$1")
    [ -n "$sum" ] && [ "$sealed" = "$sum" ]
}
# install_unit ENABLE FILE...: writes the files, then turns ENABLE on. A file that is
# not one lumen ships stays as it is, and one you turned off stays off.
install_unit() {
    on=$1
    shift
    for file in "$@"; do
        if [ -e "$units/$file" ] && ! ours "$units/$file"; then
            echo "$units/$file is not the one lumen comes with: it stays as it is"
            return 0
        fi
    done
    off=
    if [ -e "$units/$on" ] && [ "$(systemctl --user is-enabled "$on" 2>/dev/null)" = disabled ]; then
        off=1
    fi
    mkdir -p "$units"
    for file in "$@"; do
        LUMEN_EXEC=$quoted awk '
            /^ExecStart=\/usr\/bin\/lumen / { sub(/^ExecStart=\/usr\/bin\/lumen/, ""); print "ExecStart=\"" ENVIRON["LUMEN_EXEC"] "\"" $0; next }
            { print }
        ' "packaging/$file" >"$units/$file"
        printf '%s %s\n' "$seal" "$(unit_sum "$units/$file")" >>"$units/$file"
    done
    systemctl --user daemon-reload || true
    # a daemon from before makes way for the new program by itself, once it has no window open
    [ -n "$off" ] || systemctl --user enable --now "$on" || echo "$on could not be turned on: systemctl --user status $on says why"
}
if command -v systemctl >/dev/null && systemctl --user show-environment >/dev/null 2>&1; then
    [ "${LUMEN_DAEMON:-1}" = 0 ] || install_unit lumen.service lumen.service
    [ "${LUMEN_AUTO_UPDATE:-1}" = 0 ] || install_unit lumen-update.timer lumen-update.service lumen-update.timer
fi
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
