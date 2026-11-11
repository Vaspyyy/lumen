# lumen for Android

The app that opens terminals on your PC. What it does and how to use it is on the website: [lumen on your phone](https://vaspyyy.github.io/lumen/docs/phone/) and [How the phone logs in](https://vaspyyy.github.io/lumen/docs/phone-security/). This page is about building it.

## What is where

The app is two parts. The core is Rust and shared with nothing Android-specific but its entry points: it holds the SSH connection and the terminal (`alacritty_terminal`, the same engine as lumen on the PC). The rest is Kotlin and Jetpack Compose: it draws the screen the core describes, takes the touches and keys, and keeps the phone's key.

```
../crates/lumen-link/      what the PC and the phone say to each other: the pairing code, the messages
../crates/lumen-mobile/    the core
  src/link.rs              the connection: pairing, logging in, tabs
  src/screen.rs            a terminal's screen, as the app draws it
  src/jni.rs               the entry points the app calls
  examples/phone.rs        a phone on the command line, for trying the PC side without Android
  tests/with-a-pc.sh       that phone and `lumen pair`, together, against an SSH server of the test's own
app/src/main/java/io/github/vaspyyy/lumen/
  core/Core.kt             the core's entry points, with what each takes and gives
  keys/                    the key in the phone's secure hardware, and the prompt that unlocks it
  model/AppModel.kt        what the app knows: the paired PCs, the connection, the tabs
  term/TerminalView.kt     draws a terminal and takes its touches and keys
  ui/                      the screens: welcome, scanning, pairing, the terminal, the PCs
build-core.sh              builds the core into app/src/main/jniLibs
```

The phone's key never reaches the core. When the SSH login needs a signature, the core hands the bytes to sign to the app, the Android Keystore signs them inside the secure hardware, and the signature goes back.

## Build it

You need:

- a JDK, 17 or newer
- the Android SDK with the platform `android-37.0`, build tools, and an NDK (`sdkmanager "platforms;android-37.0" "ndk;27.2.12479018"`, or what Android Studio installs)
- Rust, with the targets for Android, and `cargo-ndk`:

  ```sh
  rustup target add aarch64-linux-android x86_64-linux-android
  cargo install cargo-ndk
  ```

Then, from the root of the repository:

```sh
android/build-core.sh                       # the core, for phones (arm64) and the emulator (x86_64)
cd android
./gradlew :app:assembleDebug                # app/build/outputs/apk/debug/app-debug.apk
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

`build-core.sh` finds the NDK under `$ANDROID_HOME/ndk`, or takes the one `ANDROID_NDK_HOME` names. Gradle finds the SDK through `ANDROID_HOME` or a `local.properties` with `sdk.dir=...`.

After a change to the Rust code, run `build-core.sh` again before Gradle. After a change to what `jni.rs` offers, change `Core.kt` to match: the two are kept in step by hand.

## Check it

```sh
cargo test --workspace                                              # the core's tests, on the PC
cargo build -p lumen && cargo build -p lumen-mobile --example phone
sh crates/lumen-mobile/tests/with-a-pc.sh                           # pairing, logging in, kept terminals, unpairing
cd android && ./gradlew :app:lintDebug :app:testDebugUnitTest
```

`with-a-pc.sh` starts an SSH server of its own on a port of localhost, with its own keys and its own `authorized_keys`. It reads and writes nothing of your `~/.ssh`.

To try the app in an emulator against the PC it runs on: the PC is `10.0.2.2` from inside the emulator, so pair with `lumen pair --address 10.0.2.2`. The emulator has no camera worth the name: copy the `lumen://pair/...` line into **Type the code instead**, or hand it over with `adb shell am start -a android.intent.action.VIEW -d 'lumen://pair/...'`. The emulator needs a screen lock (a PIN will do), because the key does.

## A release

A release build is shrunk by R8 and comes out unsigned unless a keystore is given:

```sh
./gradlew :app:assembleRelease     # app/build/outputs/apk/release/app-release-unsigned.apk
```

With `LUMEN_KEYSTORE`, `LUMEN_KEYSTORE_PASSWORD`, `LUMEN_KEY_ALIAS` and `LUMEN_KEY_PASSWORD` set, Gradle signs it. It never falls back to the debug key.

The APK of a release on GitHub is built by `.github/workflows/release.yml`: one job builds it unsigned, a second one, which alone can read the signing key, signs it and checks that the signature is the one [Install](https://vaspyyy.github.io/lumen/docs/install/#the-android-app) names. An APK you sign yourself has another signature, so Android won't install it over the released app, or the other way round: uninstall one first, and pair again.

The version is the one in the root `Cargo.toml`. `versionName` in `app/build.gradle.kts` has to say the same, and the release stops if it doesn't. `versionCode` is that version as a number: 0.3.0 is 300.

## Fonts

The app carries its fonts: Maple Mono for the terminal, Symbols Nerd Font for the icons tools such as `glint` print, and Geist and Instrument Serif for its own screens. Their licenses are in `licenses/`.
