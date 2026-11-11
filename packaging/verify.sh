#!/bin/sh
# Tries what a release offers the way its user would, on a clean system: it
# installs it, looks at what arrived, opens lumen on a display nobody sees,
# and removes it again. Run it as root in a container that is thrown away
# afterwards:
#
#   docker run --rm -v "$PWD:/lumen:ro" debian:12 sh /lumen/packaging/verify.sh deb
#
#   deb      the Debian package, on Debian or Ubuntu
#   rpm      the RPM, on Fedora
#   zypper   the RPM, on openSUSE
#   arch     the Arch package, on Arch
#   script   the archive and install.sh, as a user, on Debian or Ubuntu
#
# DIST names another folder than dist/ for the packages.

set -eu

kind=${1:?which one: deb, rpm, zypper, arch or script}
here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
dist=$(CDPATH='' cd -- "${DIST:-$here/../dist}" && pwd)

fail() {
    echo "verify.sh: $*" >&2
    exit 1
}

step() {
    printf '\n== %s\n' "$*"
}

# apt's lists of packages: apt-lists.sh, next to this script, keeps the
# retries and the other name for Ubuntu's archive.

case $kind in
    deb)
        export DEBIAN_FRONTEND=noninteractive
        sh "$here/apt-lists.sh" || fail "the package lists could not be fetched"
        step "install"
        apt-get install -y -q --no-install-recommends "$dist/lumen-terminal_amd64.deb"
        arrived
        step "a window"
        apt-get install -y -q --no-install-recommends xvfb xauth mesa-vulkan-drivers libgl1-mesa-dri libegl-mesa0 fonts-dejavu-core
        window lumen
        step "remove"
        apt-get remove -y -q lumen-terminal
        gone
        ;;

    rpm)
        step "install"
        dnf install -y "$dist/lumen-terminal.x86_64.rpm"
        arrived
        step "a window"
        dnf install -y xorg-x11-server-Xvfb mesa-vulkan-drivers mesa-dri-drivers dejavu-sans-mono-fonts
        window lumen
        step "remove"
        dnf remove -y lumen-terminal
        gone
        ;;

    zypper)
        step "install"
        zypper --non-interactive install --allow-unsigned-rpm "$dist/lumen-terminal.x86_64.rpm"
        arrived
        step "remove"
        zypper --non-interactive remove lumen-terminal
        gone
        ;;

    arch)
        pacman -Syu --noconfirm
        step "install"
        pacman -U --noconfirm "$dist/lumen-terminal-x86_64.pkg.tar.zst"
        arrived
        step "a window"
        pacman -S --noconfirm --needed xorg-server-xvfb mesa vulkan-swrast ttf-dejavu
        window lumen
        step "remove"
        pacman -R --noconfirm lumen-terminal
        gone
        ;;

    script)
        export DEBIAN_FRONTEND=noninteractive
        sh "$here/apt-lists.sh" || fail "the package lists could not be fetched"
        apt-get install -y -q --no-install-recommends curl ca-certificates
        useradd -m tester
        home=/home/tester
        version=$(tar -tzf "$dist/lumen-x86_64-linux.tar.gz" | sed -n '1s/^lumen-\(.*\)-x86_64-linux\/.*/\1/p')
        [ -n "$version" ] || fail "the archive's folder doesn't name a version"
        # a release as GitHub serves it, in a folder
        release=$(mktemp -d)
        cp "$dist/lumen-x86_64-linux.tar.gz" "$release/"
        (cd "$release" && sha256sum lumen-x86_64-linux.tar.gz >SHA256SUMS)
        chmod -R a+rX "$release"
        fetched="LUMEN_RELEASE_URL=file://$release"

        step "a system without the libraries: it says so and installs nothing"
        refused "su tester -c '$fetched sh $here/install.sh'" "libraries are missing"
        [ ! -e "$home/.local/bin/lumen" ] || fail "install.sh left a program behind that can't start"

        apt-get install -y -q --no-install-recommends libxkbcommon-x11-0

        step "what is loaded later is missing: it installs, and says so"
        as_tester "$fetched" >/tmp/out 2>&1 || {
            cat /tmp/out
            fail "install.sh failed"
        }
        cat /tmp/out
        grep -q "doesn't seem to have libvulkan.so.1 libEGL.so.1 libwayland-client.so.0" /tmp/out || fail "install.sh did not name what is loaded later"
        as_tester "" --uninstall

        # The fonts first, and only ones that GPUI doesn't know by name: it ends
        # the program when it finds none of a handful of common families, and
        # lumen has to start all the same.
        apt-get install -y -q --no-install-recommends fonts-liberation
        apt-get install -y -q --no-install-recommends libvulkan1 libegl1 libwayland-client0 libwayland-egl1 \
            xvfb xauth mesa-vulkan-drivers libgl1-mesa-dri libegl-mesa0 fontconfig
        known=$(fc-list : family | tr ',' '\n' | grep -x -E 'Lilex|IBM Plex Sans|Helvetica|Segoe UI|Ubuntu|Adwaita Sans|Cantarell|Noto Sans|DejaVu Sans|Arial' | sort -u | tr '\n' ' ') || true
        [ -z "$known" ] || fail "this system has $known: the start without a font GPUI knows by name can't be tried here"

        step "fetched"
        as_tester "$fetched" >/tmp/out 2>&1 || {
            cat /tmp/out
            fail "install.sh failed"
        }
        cat /tmp/out
        grep -q "^lumen $version is installed: $home/.local/bin/lumen$" /tmp/out || fail "install.sh did not say what it installed"
        ! grep -q "^Note:" /tmp/out || fail "install.sh has a note on a system that has everything"
        su tester -c "$home/.local/bin/lumen --version"
        grep -qx "Exec=$home/.local/bin/lumen" "$home/.local/share/applications/lumen.desktop" || fail "the launcher entry doesn't name the program by its path"
        [ -f "$home/.local/share/icons/hicolor/256x256/apps/lumen.png" ] || fail "the icons are missing"
        [ "$(stat -c %U "$home/.local/bin/lumen")" = tester ] || fail "the program doesn't belong to the user"
        [ "$(ls -A "$home/.local/bin")" = lumen ] || fail "something is left next to the program: $(ls -A "$home/.local/bin")"
        [ ! -e "$home/.local/share/icons/hicolor/icon-theme.cache" ] || fail "install.sh made a list of the icons"

        step "a window, with fonts that GPUI doesn't know by name"
        window "$home/.local/bin/lumen" tester

        step "the same without fc-match to ask about the fonts"
        mv /usr/bin/fc-match /usr/bin/fc-match.away
        window "$home/.local/bin/lumen" tester
        mv /usr/bin/fc-match.away /usr/bin/fc-match
        grep -q "can't be found, using" /tmp/window.log || fail "lumen did not say which font it took instead"

        step "a second time, over the first"
        as_tester "$fetched"

        step "a damaged download is refused"
        cp "$release/lumen-x86_64-linux.tar.gz" /tmp/whole.tar.gz
        printf 'x' >>"$release/lumen-x86_64-linux.tar.gz"
        refused "su tester -c '$fetched sh $here/install.sh'" "damaged"
        cp /tmp/whole.tar.gz "$release/lumen-x86_64-linux.tar.gz"

        step "as root in the user's home: refused, and nothing is made there"
        refused "HOME=$home LUMEN_BIN_DIR=$home/rootbin $fetched sh $here/install.sh" "without sudo"
        [ ! -e "$home/rootbin" ] || fail "root made a folder in the user's home"

        step "removed"
        as_tester "" --uninstall
        [ ! -e "$home/.local/bin/lumen" ] && [ ! -e "$home/.local/share/applications/lumen.desktop" ] || fail "something is left after --uninstall"

        step "another program called lumen is neither written over nor removed"
        su tester -c "printf '#!/bin/sh\necho another lumen\n' > $home/.local/bin/lumen && chmod +x $home/.local/bin/lumen"
        refused "su tester -c '$fetched sh $here/install.sh'" "another program of the same name"
        as_tester "" --uninstall
        [ "$(su tester -c "$home/.local/bin/lumen")" = "another lumen" ] || fail "the other lumen was touched"
        rm "$home/.local/bin/lumen"

        step "from the unpacked archive, into a folder with a space in its name"
        su tester -c "tar -xzf $dist/lumen-x86_64-linux.tar.gz -C $home"
        # nothing to fetch from: only the unpacked copy can be what gets installed
        su tester -c "LUMEN_RELEASE_URL=file:///nowhere LUMEN_BIN_DIR='$home/my bin' sh $home/lumen-$version-x86_64-linux/install.sh" >/tmp/out 2>&1 || {
            cat /tmp/out
            fail "install.sh failed from the unpacked archive"
        }
        cat /tmp/out
        grep -q "^lumen $version is installed: $home/my bin/lumen$" /tmp/out || fail "install.sh did not install the unpacked copy"
        grep -x "Exec=\"$home/my bin/lumen\"" "$home/.local/share/applications/lumen.desktop" || fail "the path in the launcher entry is not quoted"
        su tester -c "'$home/my bin/lumen' --version"

        step "removed without naming the folder: the launcher entry says where the program is"
        as_tester "" --uninstall
        [ ! -e "$home/my bin/lumen" ] && [ ! -e "$home/.local/share/applications/lumen.desktop" ] || fail "something is left after --uninstall"

        step "nothing there: it says so"
        as_tester "" --uninstall >/tmp/out 2>&1
        cat /tmp/out
        grep -q "There is no lumen in" /tmp/out || fail "--uninstall did not say that there was nothing to remove"
        ;;

    *)
        fail "unknown kind: $kind"
        ;;
esac

printf '\nverify.sh: %s is fine\n' "$kind"
