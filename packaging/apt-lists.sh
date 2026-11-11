#!/bin/sh
# Fetches apt's lists of packages for the workflows and for verify.sh, and
# doesn't take one archive's DNS trouble for "no such package": apt by itself
# goes on without the lists, and then says of everything that it can't be
# installed, which sends whoever reads the log after the wrong thing.
#
#   sudo sh packaging/apt-lists.sh
#
# A source that doesn't answer is tried once more, and then the same archive
# is asked under another name of its own: on 2026-11-11 GitHub's runners could
# not resolve archive.ubuntu.com (and azure.archive.ubuntu.com) for hours,
# while the containers' deb.debian.org and Fedora's and Arch's mirrors
# answered in the same run.

set -u

# Apt says when a source failed only when it is told to; by itself it goes on.
# A look-up that hangs would take minutes per source, so one try is held to
# three minutes, which is far more than a fetch of the lists needs.
lists() {
    timeout 180 apt-get update -q -o APT::Update::Error-Mode=any
}

# The same archive under a name of its own. Only ubuntu.com names are
# replaced, so on Debian and everywhere else this changes nothing at all.
# Ubuntu 26's runners keep their mirrors in a list of their own, which the
# sources point at instead of naming a host.
mirror() {
    for file in /etc/apt/sources.list /etc/apt/sources.list.d/*.sources /etc/apt/apt-mirrors.txt; do
        [ -e "$file" ] || continue
        sed -i -e 's|//archive\.ubuntu\.com/ubuntu|//mirrors.edge.kernel.org/ubuntu|g' \
               -e 's|//security\.ubuntu\.com/ubuntu|//mirrors.edge.kernel.org/ubuntu|g' \
               -e 's|//azure\.archive\.ubuntu\.com/ubuntu|//mirrors.edge.kernel.org/ubuntu|g' "$file"
    done
}

lists && exit 0

echo "apt-lists.sh: the package lists could not be fetched; trying the same archive under another name" >&2
mirror
lists && exit 0

echo "apt-lists.sh: the package lists could not be fetched; trying again in 20s" >&2
sleep 20
lists && exit 0

echo "apt-lists.sh: the package lists could not be fetched" >&2
exit 1
