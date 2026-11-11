#!/bin/sh
# The phone's side and the PC's side, together: a phone on the command line
# (examples/phone.rs) is paired with `lumen pair`, logs in, runs a command in
# a terminal the PC keeps, goes away, comes back to find it, pairs again
# with a new key, and is unpaired.
#
# It runs against an SSH server of its own, started as the current user on
# a port of localhost, with its own host key and its own authorized_keys:
# nothing of the system's SSH or of ~/.ssh is read or written.
#
#   cargo build -p lumen && cargo build -p lumen-mobile --example phone
#   sh crates/lumen-mobile/tests/with-a-pc.sh
#
# LUMEN_BIN and PHONE_BIN name other programs than the debug builds.
# Without sshd, ssh-keygen or openssl it says so and passes.

set -eu
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../../.." && pwd)
lumen=${LUMEN_BIN:-$root/target/debug/lumen}
phone=${PHONE_BIN:-$root/target/debug/examples/phone}

sshd=$(command -v sshd || true)
[ -n "$sshd" ] || sshd=$(ls /usr/sbin/sshd /usr/bin/sshd 2>/dev/null | head -n 1 || true)
for tool in "$sshd" ssh-keygen openssl; do
    if [ -z "$tool" ] || ! command -v "$tool" >/dev/null; then
        echo "with-a-pc.sh: skipped, this system has no ${tool:-sshd}"
        exit 0
    fi
done
[ -x "$lumen" ] && [ -x "$phone" ] || {
    echo "with-a-pc.sh: build lumen and the phone example first (see the top of this file)" >&2
    exit 1
}

work=$(mktemp -d)
run=$work/run
mkdir -m 700 "$run" "$work/phone"
server=""
pairing=""

fail() {
    echo "with-a-pc.sh: $*" >&2
    exit 1
}

step() {
    printf '\n== %s\n' "$*"
}

cleanup() {
    status=$?
    XDG_RUNTIME_DIR=$run "$lumen" sessions kill --all >/dev/null 2>&1 || true
    [ -z "$pairing" ] || kill "$pairing" 2>/dev/null || true
    [ -z "$server" ] || kill "$server" 2>/dev/null || true
    if [ "$status" != 0 ] && [ -f "$work/sshd.log" ]; then
        echo "-- what the SSH server said:" >&2
        tail -n 20 "$work/sshd.log" >&2
    fi
    rm -rf "$work"
    exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT TERM HUP

ssh-keygen -q -t ed25519 -N '' -f "$work/hostkey"
: >"$work/authorized_keys"

# A free port: the server itself says whether it got one.
for try in 1 2 3 4 5; do
    port=$((20000 + $(od -An -N2 -tu2 /dev/urandom | tr -d ' ') % 30000))
    # What a login through this server starts finds the test's own runtime
    # folder, and a shell that greets with nothing but a prompt.
    "$sshd" -D -e -f /dev/null -p "$port" -o ListenAddress=127.0.0.1 -o HostKey="$work/hostkey" \
        -o AuthorizedKeysFile="$work/authorized_keys" -o PidFile=none -o UsePAM=no -o StrictModes=no \
        -o PasswordAuthentication=no -o KbdInteractiveAuthentication=no \
        -o SetEnv="XDG_RUNTIME_DIR=$run LUMEN_SESSION_SHELL=/bin/sh" 2>"$work/sshd.log" &
    server=$!
    sleep 1
    if kill -0 "$server" 2>/dev/null; then
        break
    fi
    server=""
    [ "$try" != 5 ] || fail "the SSH server does not start: $(tail -n 3 "$work/sshd.log")"
done
export XDG_RUNTIME_DIR="$run"

pair() {
    TZ=Pacific/Kiritimati "$lumen" pair --yes --port "$port" --address 127.0.0.1 --host-key "$work/hostkey.pub" \
        --authorized-keys "$work/authorized_keys" >"$work/link" 2>"$work/pair.log" &
    pairing=$!
    for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
        grep -q '^lumen://pair/' "$work/link" && return 0
        sleep 0.25
    done
    cat "$work/pair.log" >&2
    fail "lumen pair showed no code"
}

step "pair"
pair
# the code is the first line; what lumen pair says afterwards follows it
code=$(head -n 1 "$work/link")
grep -q 'lumen-pairing:' "$work/authorized_keys" || fail "the short-lived key is not in authorized_keys"
# The key's time is up in ten minutes by the system's clock, whatever time
# zone the shell that ran lumen pair is in: sshd reads it by the system's.
expiry=$(sed -n 's/.*expiry-time="\([0-9]*\)".*/\1/p' "$work/authorized_keys")
soon=$(env -u TZ date -d '+9 minutes' +%Y%m%d%H%M%S)
late=$(env -u TZ date -d '+11 minutes' +%Y%m%d%H%M%S)
[ "$(expr "$expiry" \> "$soon")" = 1 ] && [ "$(expr "$expiry" \< "$late")" = 1 ] ||
    fail "the short-lived key expires at $expiry, not in ten minutes (between $soon and $late)"
"$phone" enroll "$work/phone" "$code" "test phone" >"$work/host.json" || fail "the phone could not pair"
wait "$pairing" || fail "lumen pair did not end well: $(cat "$work/pair.log")"
pairing=""
cat "$work/authorized_keys"
grep -q 'lumen-phone:test phone$' "$work/authorized_keys" || fail "the phone's key is not in authorized_keys"
! grep -q 'lumen-pairing:' "$work/authorized_keys" || fail "the short-lived key is still in authorized_keys"
[ "$(wc -l <"$work/authorized_keys")" = 1 ] || fail "authorized_keys holds more than the phone's line"

step "the code works once"
if "$phone" enroll "$work/phone" "$code" "second phone" >"$work/out" 2>&1; then
    fail "the same code paired a second phone"
fi
cat "$work/out"
grep -q 'refused' "$work/out" || fail "a used code was not refused"

step "log in: the PC keeps terminals"
"$phone" sessions "$work/phone" >"$work/out" || fail "the phone could not log in"
cat "$work/out"
grep -qx 'keeps: true' "$work/out" || fail "the phone did not find lumen on the PC"

step "a terminal: type, see, leave"
"$phone" run "$work/phone" t1 "printf 'marker-%s\n' 42" marker-42 >"$work/out" || fail "no terminal"
cat "$work/out"
"$phone" sessions "$work/phone" >"$work/out"
cat "$work/out"
grep -q '^t1 attached=0 ' "$work/out" || fail "the PC does not keep the terminal the phone left"

step "come back: what was on screen is there again"
"$phone" run "$work/phone" t1 "printf 'again-%s\n' 7" again-7 >"$work/out" || fail "could not come back"
cat "$work/out"
grep -q '^marker-42$' "$work/out" || fail "the screen from before did not come back"

step "end the terminal"
"$phone" kill "$work/phone" t1
"$phone" sessions "$work/phone" >"$work/out"
! grep -q '^t1 ' "$work/out" || fail "the terminal is still there"

step "pair again with a new key: it takes the place of the old one"
mkdir -m 700 "$work/phone2"
pair
code=$(head -n 1 "$work/link")
PHONE_HAD=$("$phone" key "$work/phone") "$phone" enroll "$work/phone2" "$code" "test phone" >/dev/null ||
    fail "the phone could not pair again"
wait "$pairing" || fail "lumen pair did not end well: $(cat "$work/pair.log")"
pairing=""
cat "$work/authorized_keys"
[ "$(wc -l <"$work/authorized_keys")" = 1 ] || fail "the line of the old key is still in authorized_keys"
grep -q 'lumen-phone:test phone$' "$work/authorized_keys" || fail "the phone did not keep its name"
if "$phone" sessions "$work/phone" >"$work/out" 2>&1; then
    fail "the key that was replaced still gets in"
fi
"$phone" sessions "$work/phone2" >"$work/out" || fail "the new key does not get in"

step "unpair"
"$lumen" unpair "test phone" --authorized-keys "$work/authorized_keys"
if "$phone" sessions "$work/phone2" >"$work/out" 2>&1; then
    fail "an unpaired phone still gets in"
fi
cat "$work/out"
grep -q 'refused' "$work/out" || fail "an unpaired phone was not refused"

printf '\nwith-a-pc.sh: fine\n'
