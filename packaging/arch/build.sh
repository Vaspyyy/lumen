#!/bin/sh
# Makes the Arch package from the archive that package.sh made. Runs on Arch:
# as yourself, or as root in a container that is thrown away afterwards (it
# adds a user there, since makepkg doesn't work as root).
#
#   packaging/arch/build.sh
#
# DIST names another folder than dist/ for the archive and the result.

set -eu
CDPATH='' cd -- "$(dirname -- "$0")/../.."

fail() {
    echo "arch/build.sh: $*" >&2
    exit 1
}

dist=$(realpath "${DIST:-dist}")
archive=$dist/lumen-x86_64-linux.tar.gz
[ -f "$archive" ] || fail "no archive at $archive: run packaging/package.sh first"
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
sum=$(sha256sum "$archive" | cut -d' ' -f1)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp packaging/arch/PKGBUILD "$work/"
# makepkg takes a file that lies next to the PKGBUILD instead of fetching it
cp "$archive" "$work/lumen-$version-x86_64-linux.tar.gz"
sed -i "s/^pkgver=.*/pkgver=$version/; s/^sha256sums=.*/sha256sums=('$sum')/" "$work/PKGBUILD"

# what lumen needs to run doesn't have to be installed to wrap it up
if [ "$(id -u)" = 0 ]; then
    id builder >/dev/null 2>&1 || useradd -m builder
    chown -R builder "$work"
    su builder -c "cd '$work' && PKGEXT=.pkg.tar.zst makepkg --nodeps"
else
    (cd "$work" && PKGEXT=.pkg.tar.zst makepkg --nodeps)
fi
cp "$work"/lumen-terminal-"$version"-*.pkg.tar.zst "$dist/lumen-terminal-x86_64.pkg.tar.zst"
ls -l "$dist/lumen-terminal-x86_64.pkg.tar.zst"
