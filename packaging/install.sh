#!/bin/sh
# Installs lumen for one user, without root: the program into ~/.local/bin,
# the launcher entry and the icons into ~/.local/share.
#
#   curl -fsSL https://vaspyyy.github.io/lumen/install.sh | sh
#
# Started from an unpacked release it installs what lies next to it. Started
# any other way it fetches the newest release first, and checks that it
# arrived whole. To remove lumen again:
#
#   curl -fsSL https://vaspyyy.github.io/lumen/install.sh | sh -s -- --uninstall
#
# LUMEN_VERSION=0.2.2 picks a version, LUMEN_BIN_DIR another folder for the
# program.

set -eu

repo=https://github.com/Vaspyyy/lumen
docs=https://vaspyyy.github.io/lumen/docs/install/
sizes="32 48 64 128 256 512"

say() {
    printf '%s\n' "$*"
}

die() {
    printf 'lumen: %s\n' "$*" >&2
    exit 1
}

usage() {
    say "Installs lumen for this user, into ~/.local. No root is needed."
    say ""
    say "    sh install.sh               install, or update"
    say "    sh install.sh --uninstall   remove it again"
    say ""
    say "LUMEN_VERSION=0.2.2 picks a version, LUMEN_BIN_DIR another folder for the program."
}

# Whether the program at $1 is this lumen. Other programs are called lumen
# too, and one of them may be what is in the folder.
ours() {
    grep -q -F 'lumen: a terminal emulator' "$1" 2>/dev/null
}

# The same question for a launcher entry.
our_entry() {
    grep -q '^Icon=lumen$' "$1" 2>/dev/null && grep -q '^Categories=.*TerminalEmulator' "$1" 2>/dev/null
}

# Some desktops keep a list of the launcher entries.
refresh() {
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$data_dir/applications" >/dev/null 2>&1 || true
    fi
    # A list of the icons, where a desktop keeps one, is taken as it is for as
    # long as the folder is no newer than it: the folder gets a new date, so
    # that the icons are looked up afresh. Making such a list here would hide
    # the icons that other programs add later.
    touch "$data_dir/icons/hicolor" 2>/dev/null || true
}

uninstall() {
    program=$bin_dir/lumen
    entry=$data_dir/applications/lumen.desktop
    # installed into another folder? The launcher entry names the program.
    if [ ! -e "$program" ] && [ -z "${LUMEN_BIN_DIR:-}" ] && our_entry "$entry"; then
        named=$(sed -n 's/^Exec=//p' "$entry" | head -n 1)
        case $named in
            '"'*'"')
                named=${named#\"}
                named=${named%\"}
                ;;
        esac
        case $named in
            *\\*) ;;
            /*/lumen) program=$named ;;
        esac
    fi

    found=
    other=
    if [ -e "$program" ]; then
        if ours "$program"; then
            rm -f -- "$program"
            found=1
        else
            say "$program is another program of the same name. It stays."
            other=1
        fi
    fi
    if [ -e "$entry" ]; then
        if our_entry "$entry"; then
            rm -f -- "$entry"
            for size in $sizes; do
                rm -f -- "$data_dir/icons/hicolor/${size}x${size}/apps/lumen.png"
            done
            found=1
        else
            say "$entry belongs to another program of the same name. It stays."
            other=1
        fi
    fi
    if [ -n "$found" ]; then
        refresh
        say "lumen is removed. Your settings are still in ${XDG_CONFIG_HOME:-$HOME/.config}/lumen."
    elif [ -z "$other" ]; then
        say "There is no lumen in $bin_dir. If it went into another folder, name it with LUMEN_BIN_DIR."
    fi
}

# fetch URL FILE
fetch() {
    # a release is fetched over https and nothing else; a test may name a folder
    case $1 in
        https://*) protocols='=https' ;;
        *) protocols='=https,file' ;;
    esac
    if command -v curl >/dev/null 2>&1; then
        curl --proto "$protocols" --tlsv1.2 -fsSL -o "$2" "$1"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$2" "$1"
    else
        die "curl or wget is needed to fetch lumen"
    fi
}

# Fetches a release and unpacks it. Leaves the folder it is in as $from.
download() {
    case $(uname -m) in
        x86_64 | amd64) machine=x86_64 ;;
        *) die "there is no prebuilt lumen for $(uname -m) yet. It can be built from source: $docs" ;;
    esac
    command -v tar >/dev/null 2>&1 || die "tar is needed to unpack lumen"
    command -v sha256sum >/dev/null 2>&1 || die "sha256sum is needed to check what was fetched"

    if [ -n "${LUMEN_RELEASE_URL:-}" ]; then
        base=$LUMEN_RELEASE_URL
    elif [ -n "${LUMEN_VERSION:-}" ]; then
        base=$repo/releases/download/v${LUMEN_VERSION#v}
    else
        base=$repo/releases/latest/download
    fi
    archive=lumen-$machine-linux.tar.gz

    # Unpacked next to where the program goes, not in /tmp: lumen is started
    # once before it is installed, and /tmp may not allow that.
    mkdir -p -- "$bin_dir" || die "can't make the folder $bin_dir"
    tmp=$(mktemp -d "$bin_dir/.lumen-install.XXXXXX") || die "can't write to $bin_dir"
    trap 'rm -rf -- "$tmp"' EXIT
    trap 'exit 130' INT TERM HUP

    say "Fetching $base/$archive"
    fetch "$base/$archive" "$tmp/$archive" || die "could not fetch $base/$archive"
    fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || die "could not fetch $base/SHA256SUMS"

    want=$(awk -v name="$archive" '$2 == name || $2 == "*" name { print $1 }' "$tmp/SHA256SUMS")
    have=$(sha256sum "$tmp/$archive" | awk '{ print $1 }')
    [ -n "$want" ] || die "the release's SHA256SUMS doesn't list $archive"
    [ "$want" = "$have" ] || die "what was fetched is damaged: its checksum is $have, the release says $want"

    tar -xzf "$tmp/$archive" -C "$tmp"
    from=
    for folder in "$tmp"/lumen-*-linux; do
        [ -d "$folder" ] && from=$folder
    done
    [ -n "$from" ] && [ -x "$from/lumen" ] || die "the archive doesn't hold what was expected"
}

# Says why the program in $1 can't start here, and stops.
cannot_start() {
    missing=$(ldd "$1" 2>/dev/null | awk '/=> not found/ { printf "%s ", $1 }') || true
    if [ -n "$missing" ]; then
        die "lumen can't start here, these libraries are missing: $missing
Nothing was installed. What lumen needs: $docs"
    fi
    if ldd "$1" 2>&1 | grep -q 'GLIBC_'; then
        die "lumen can't start here: this system's C library is older than the one lumen is built for (glibc 2.35, as in Ubuntu 22.04).
Nothing was installed. lumen can be built from source: $docs"
    fi
    die "lumen can't start here. Nothing was installed. What lumen needs: $docs"
}

# What lumen loads only when it opens a window: it starts without these, and
# then has nothing to draw with. Prints the ones this system doesn't have.
not_loadable() {
    lister=
    for candidate in ldconfig /sbin/ldconfig /usr/sbin/ldconfig; do
        if command -v "$candidate" >/dev/null 2>&1; then
            lister=$candidate
            break
        fi
    done
    [ -n "$lister" ] || return 0
    known=$("$lister" -p 2>/dev/null) || return 0
    [ -n "$known" ] || return 0
    for library in libvulkan.so.1 libEGL.so.1 libwayland-client.so.0; do
        case $known in
            *"$library"*) ;;
            *) printf '%s ' "$library" ;;
        esac
    done
}

# A path as the Exec line of a launcher entry wants it: as it is when it is
# plain, in quotes when it has a space or another character with a meaning.
desktop_quote() {
    case $1 in
        *[!A-Za-z0-9/._+-]*) ;;
        *)
            printf '%s' "$1"
            return
            ;;
    esac
    printf '"%s"' "$(printf '%s' "$1" | sed -e 's/\\/\\\\\\\\/g' -e 's/["`$]/\\\\&/g' -e 's/%/%%/g')"
}

# Installs the release that is unpacked in $1.
install_from() {
    "$1/lumen" --version >/dev/null 2>&1 || cannot_start "$1/lumen"
    version=$("$1/lumen" --version | awk '{ print $2 }')

    # nothing of another program of the same name is written over
    entry=$data_dir/applications/lumen.desktop
    if [ -e "$bin_dir/lumen" ] && ! ours "$bin_dir/lumen"; then
        die "$bin_dir/lumen is another program of the same name, and it stays as it is.
Nothing was installed. LUMEN_BIN_DIR names another folder for this lumen."
    fi
    if [ -e "$entry" ] && ! our_entry "$entry"; then
        die "$entry belongs to another program of the same name, and it stays as it is.
Nothing was installed."
    fi

    # A lumen that is running keeps the program it was started from: the new
    # one is put beside it and then takes its name.
    mkdir -p -- "$bin_dir"
    cp -- "$1/lumen" "$bin_dir/.lumen.new"
    chmod 755 "$bin_dir/.lumen.new"
    mv -f -- "$bin_dir/.lumen.new" "$bin_dir/lumen"

    # The launcher entry names the program by its whole path: a desktop doesn't
    # always have ~/.local/bin on its PATH.
    mkdir -p -- "$data_dir/applications"
    LUMEN_EXEC=$(desktop_quote "$bin_dir/lumen") awk '
        /^Exec=lumen/ { sub(/^Exec=lumen/, ""); print "Exec=" ENVIRON["LUMEN_EXEC"] $0; next }
        { print }
    ' "$1/lumen.desktop" >"$entry"

    for size in $sizes; do
        mkdir -p -- "$data_dir/icons/hicolor/${size}x${size}/apps"
        cp -- "$1/icons/lumen-$size.png" "$data_dir/icons/hicolor/${size}x${size}/apps/lumen.png"
    done
    refresh

    say "lumen $version is installed: $bin_dir/lumen"
    first=$(command -v lumen 2>/dev/null) || first=
    if [ -z "$first" ]; then
        say "$bin_dir is not on your PATH. Add it, or start lumen from your desktop's menu."
    elif [ "$first" != "$bin_dir/lumen" ]; then
        say "Another lumen comes first on your PATH: $first"
    fi
    absent=$(not_loadable)
    if [ -n "$absent" ]; then
        say "Note: this system doesn't seem to have $absent"
        say "lumen draws with Vulkan or EGL, and needs the Wayland library on Wayland. See $docs"
    fi
}

main() {
    case ${1:-} in
        -h | --help)
            usage
            return
            ;;
        '' | --uninstall) ;;
        *) die "unknown option: $1 (try --help)" ;;
    esac

    [ "$(uname -s)" = Linux ] || die "lumen runs on Linux. This is $(uname -s)."
    case ${HOME:-} in
        /*) ;;
        *) die "HOME is not set to a folder, so there is no place to install to" ;;
    esac
    bin_dir=${LUMEN_BIN_DIR:-$HOME/.local/bin}
    case $bin_dir in
        /*) ;;
        *) die "LUMEN_BIN_DIR has to be a whole path, starting at /: $bin_dir" ;;
    esac
    # a path that doesn't start at / is no path for this, the desktop's rules say
    case ${XDG_DATA_HOME:-} in
        /*) data_dir=$XDG_DATA_HOME ;;
        *) data_dir=$HOME/.local/share ;;
    esac

    if [ "$(id -u)" = 0 ]; then
        # sudo can leave HOME as it was: root would then make folders in your
        # home that you can't write to any more
        owner=$(stat -c %u -- "$HOME" 2>/dev/null) || owner=0
        [ "$owner" = 0 ] || die "this was started as root, in the home folder of another user ($HOME).
Run it as yourself, without sudo: it needs no root."
        [ "${1:-}" = --uninstall ] || say "Note: this installs lumen for the user root only."
    fi

    if [ "${1:-}" = --uninstall ]; then
        uninstall
        return
    fi

    # next to an unpacked release? then that release is the one to install
    here=
    case $0 in
        */install.sh) here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd) ;;
        install.sh) here=$(pwd) ;;
    esac
    if [ -n "$here" ] && [ -x "$here/lumen" ] && [ -f "$here/lumen.desktop" ]; then
        install_from "$here"
    else
        download
        install_from "$from"
    fi
}

# Everything above only defines; this line starts it. A script that arrived
# only in part does nothing.
main "$@"
