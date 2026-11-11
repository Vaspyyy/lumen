#!/bin/sh
# Fetches apt's lists of packages for the workflows and for verify.sh, and
# doesn't take one archive's DNS trouble for "no such package": apt by itself
# goes on without the lists, and then says of everything that it can't be
# installed, which sends whoever reads the log after the wrong thing.
#
#   sudo sh packaging/apt-lists.sh
#
# A source that doesn't answer for a moment is tried again. If it still
# doesn't, Ubuntu's archive is asked under another name of its own: on
# 2026-11-11 GitHub's runners could not resolve archive.ubuntu.com for hours,
# while deb.debian.org and Fedora's and Arch's mirrors answered in the same
# run.

set -u

# apt says when a source failed only when it is told to; by itself it goes on.
lists() {
    apt-get update -q -o APT::Update::Error-Mode=any
}

# The same archive under a name of its own. Only ubuntu.com names are
# replaced, so on Debian and everywhere else this changes nothing at all.
mirror() {
    for file in /etc/apt/sources.list /etc/apt/sources.list.d/*.sources; do
        [ -e "$file" ] || continue
        sed -i -e 's|//archive\.ubuntu\.com/ubuntu|//mirrors.edge.kernel.org/ubuntu|g' \
               -e 's|//security\.ubuntu\.com/ubuntu|//mirrors.edge.kernel.org/ubuntu|g' "$file"
    done
}

try() {
    for wait in 5 15 30 60 0; do
        lists && return 0
        if [ "$wait" != 0 ]; then
            echo "apt-lists.sh: the package lists could not be fetched; again in ${wait}s" >&2
            sleep "$wait"
        fi
    done
    return 1
}

try && exit 0

echo "apt-lists.sh: trying the same archive under another name" >&2
mirror
try && exit 0

echo "apt-lists.sh: the package lists could not be fetched" >&2
exit 1
