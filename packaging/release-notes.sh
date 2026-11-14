#!/bin/sh
# Prints the note of a version as the release page shows it: the text from the
# website without its head, with links that lead to the website.
#
#   packaging/release-notes.sh 0.2.2          a version of lumen
#   packaging/release-notes.sh glint 0.11.3   a version of one of its tools
#
# A tool's GitHub release can take its text from here too:
#
#   packaging/release-notes.sh glint 0.11.3 | gh release create v0.11.3 -R Vaspyyy/glint -F -

set -eu
CDPATH='' cd -- "$(dirname -- "$0")/.."

site=https://vaspyyy.github.io/lumen
if [ $# -ge 2 ]; then
    note=site/src/content/tool-releases/$1/$2.md
    install="\`go install github.com/Vaspyyy/$1@latest\`, and all of $1's release notes are at $site/releases/$1/"
else
    note=site/src/content/releases/${1:?which version}.md
    install=$site/docs/install/
fi
[ -f "$note" ] || {
    echo "release-notes.sh: no note at $note" >&2
    exit 1
}

# the head is what stands between the first two lines of three dashes
awk 'head < 2 && /^---$/ { head++; next } head >= 2 { print }' "$note" |
    sed -e '1{/^$/d;}' -e "s#](/#]($site/#g"
printf '\n---\n\nHow to install it: %s\n' "$install"
