#!/bin/sh
# Measures lumen's lag without a window on the desktop: builds lumen with the
# lag recorder (src/lag.rs), lets it type its scripted scenario into the shell
# on a display nobody sees (a headless gamescope), and prints the report
# (scripts/lag.py). Nothing typed is run: the scenario never presses Enter.
#
#   scripts/lag.sh [record.jsonl]
#
# LAG_HZ is the hidden display's refresh rate (165). To compare two builds,
# run this in each checkout, then: scripts/lag.py before.jsonl after.jsonl
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
target=${CARGO_TARGET_DIR:-$root/target}
record=${1:-$target/lag/$(date +%Y%m%d-%H%M%S).jsonl}
hz=${LAG_HZ:-165}

if ! command -v gamescope >/dev/null 2>&1; then
    echo "lag.sh: needs gamescope, whose headless backend is the display nobody sees" >&2
    exit 1
fi
# on a loaded machine the numbers would be the machine's, not lumen's
if command -v pulse >/dev/null 2>&1; then
    load=$(pulse load 2>/dev/null || true)
    case $load in
        busy*)
            echo "lag.sh: $load" >&2
            echo "lag.sh: measure again once the machine is idle" >&2
            exit 1
            ;;
    esac
fi

mkdir -p "$(dirname "$record")"
record=$(cd "$(dirname "$record")" && pwd)/$(basename "$record")
cargo build --release --features lag --manifest-path "$root/Cargo.toml"

# Wayland, as on a desktop. The window has the focus in gamescope, so GPUI
# doesn't hold its frames back as it does for a window in the background.
if ! timeout 120 gamescope --backend headless -W 1600 -H 1000 -r "$hz" --expose-wayland -- \
    env LUMEN_LAG="$record" LUMEN_LAG_DRIVE=1 "$target/release/lumen" -d "${TMPDIR:-/tmp}" \
    >"$record.log" 2>&1; then
    echo "lag.sh: the run failed, its output is in $record.log" >&2
    exit 1
fi
python3 "$root/scripts/lag.py" "$record"
echo "the record: $record"
