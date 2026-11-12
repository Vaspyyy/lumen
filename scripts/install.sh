#!/bin/sh
# Builds lumen and installs it for this user: the program into Cargo's bin
# folder, the launcher entry and the icons into ~/.local/share, and the
# systemd user service that keeps lumen waiting from login on (LUMEN_DAEMON=0
# leaves it out).
set -e
CDPATH='' cd -- "$(dirname -- "$0")/.."
cargo install --path . --locked
program=${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}/bin/lumen
data=${XDG_DATA_HOME:-$HOME/.local/share}
install -Dm644 packaging/lumen.desktop "$data/applications/lumen.desktop"
# without systemd for this user (over SSH, say) there is nothing to turn it on with
if [ "${LUMEN_DAEMON:-1}" != 0 ] && command -v systemctl >/dev/null && systemctl --user show-environment >/dev/null 2>&1; then
    units=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user
    # a service you turned off stays off
    off=
    if [ -e "$units/lumen.service" ] && [ "$(systemctl --user is-enabled lumen.service 2>/dev/null)" = disabled ]; then
        off=1
    fi
    mkdir -p "$units"
    # quoted as systemd reads a path: \ and " escaped, % and $ doubled
    quoted=$(printf '%s' "$program" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' -e 's/%/%%/g' -e 's/\$/$$/g')
    LUMEN_EXEC=$quoted awk '
        /^ExecStart=\/usr\/bin\/lumen / { sub(/^ExecStart=\/usr\/bin\/lumen/, ""); print "ExecStart=\"" ENVIRON["LUMEN_EXEC"] "\"" $0; next }
        { print }
    ' packaging/lumen.service >"$units/lumen.service"
    systemctl --user daemon-reload || true
    # a daemon from before makes way for the new program by itself, once it has no window open
    [ -n "$off" ] || systemctl --user enable --now lumen.service || echo "the service could not be started: systemctl --user status lumen says why"
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
