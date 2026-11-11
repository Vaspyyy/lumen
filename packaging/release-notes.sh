#!/bin/sh
# Prints the note of a version as the release page shows it: the text from the
# website without its head, with links that lead to the website.
#
#   packaging/release-notes.sh 0.2.2

set -eu
CDPATH='' cd -- "$(dirname -- "$0")/.."

site=https://vaspyyy.github.io/lumen
note=site/src/content/releases/${1:?which version}.md
[ -f "$note" ] || {
    echo "release-notes.sh: no note at $note" >&2
    exit 1
}

# the head is what stands between the first two lines of three dashes
awk 'head < 2 && /^---$/ { head++; next } head >= 2 { print }' "$note" |
    sed -e '1{/^$/d;}' -e "s#](/#]($site/#g"
printf '\n---\n\nHow to install it: %s/docs/install/\n' "$site"
