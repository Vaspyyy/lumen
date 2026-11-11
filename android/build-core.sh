#!/bin/sh
# Builds lumen's core (crates/lumen-mobile: the terminal and the connection
# to the PC, in Rust) for Android and puts it where Gradle packs it into the
# app: app/src/main/jniLibs/<abi>/liblumen_mobile.so.
#
#   android/build-core.sh            for phones (arm64) and the emulator (x86_64)
#   android/build-core.sh arm64-v8a  for phones only, as a release has it
#
# Wants cargo-ndk (cargo install cargo-ndk), the Rust targets
# aarch64-linux-android and x86_64-linux-android, and the NDK: ANDROID_NDK_HOME,
# or the newest one under $ANDROID_HOME/ndk.

set -eu
CDPATH='' cd -- "$(dirname -- "$0")/.."

if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    sdk=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}
    ndk=$(find "$sdk/ndk" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort -V | tail -n 1)
    [ -n "$ndk" ] || {
        echo "build-core.sh: no NDK under $sdk/ndk: set ANDROID_NDK_HOME" >&2
        exit 1
    }
    export ANDROID_NDK_HOME="$ndk"
fi

targets=""
for abi in ${*:-arm64-v8a x86_64}; do
    targets="$targets -t $abi"
done

# 30 is the oldest Android the app runs on (minSdk)
# shellcheck disable=SC2086
cargo ndk $targets --platform 30 -o android/app/src/main/jniLibs build -p lumen-mobile --release --locked
ls -l android/app/src/main/jniLibs/*/
