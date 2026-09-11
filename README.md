# androidemu — Emulia

A NES emulator for the OnePlus Pad 3, written from scratch in Rust with an Android
Compose shell. No ads, accounts, feature locks, bundled ROMs or network permission.

The same core also runs on **macOS and Linux**, with the whole shell rather than
a player: the box-art shelf, ten save slots, rewind and fast-forward, the ten
looks and palettes, and gamepads with a mapping wizard, in SDL2, OpenGL and egui.
See [desktop build instructions, controls and data layout](docs/DESKTOP.md):

```sh
cargo run --release -p nes-desktop -- "/path/to/game.nes"
```

Ships as **Emulia**, package `com.bsteinfeld.emulia`. `androidemu` is the
repository, and `dev.androidemu` stays the Kotlin package because it is the JNI
symbol prefix. The shipping name deliberately carries no third-party trademark.

[PLAN.md](PLAN.md) is the full plan. [Phase 1 acceptance](docs/PHASE-1.md) and
[the Phase 2 record](docs/PHASE-2.md) record what is implemented, what is tested and
what still needs device validation.

## Status

**Playing on the tablet; the measurements are still outstanding.** Every release
is built, tested, signed and published by GitHub Actions — see
[the release runbook](docs/RELEASING.md).

| Component | Current implementation |
|---|---|
| CPU and bus | All opcodes, cycle-based accesses, interrupt polling, OAM and DMC DMA |
| Cartridges | iNES/NES 2.0, payload identity, header repair and override table, mappers 0/1/2/3/4/7/9/10/11/66/69/71, battery RAM |
| PPU | Background/sprite pixels, scrolling, clipping, priority, sprite 0 hit, NTSC/PAL/Dendy timing |
| APU | Five channels, nonlinear mixer, FIR anti-aliasing, 48 kHz samples |
| Audio out | AAudio low-latency, adaptive queue target, dynamic rate control, latency shown in Settings (~12–14 ms; [why not less](docs/AUDIO.md)) |
| Persistence | Versioned deterministic states, validated transactional restore, SRAM, rewind |
| Android | Box-art shelf, ROM import, GLES video, AAudio, full screen, touch and two controller ports with a mapping wizard, one design system for colour, shape and buttons |
| Identity | One mark on the launcher, the round and themed icons, the splash and the empty shelf, generated from [`brand/`](brand/) and measured against eight checks |
| Time controls | One draggable control for rewind and fast-forward, further from centre is faster, plus 5 s / 15 s jumps back; shoulder buttons drive it too |
| Picture | 4:3 / 8:7 / pixel-perfect shapes, overscan trim, ten looks from scanlines to a cel-shaded Cartoon, and a separate choice of colours with `.pal` import — previewed in settings by the real shader |
| Save UI | Ten slots, thumbnails, timestamps, overwrite confirmation, autosave on pause, background and low battery; Reset game in the pause menu keeps battery saves and manual slots |
| Shelf | Chosen box art per game, last-played ordering, playtime, screenshots to the device gallery, a readable problem log |

The 39-ROM CPU/APU/PPU/MMC3 regression set and the real `nestest` trace pass.
AccuracyCoin now passes **144/144 tests**, up from 92/144; all 144 complete. See the
[accuracy report and limitations](docs/ACCURACY.md).
Device performance and latency are not yet measured. Sprite evaluation, some DMA
edge cases, PAL/Dendy, raw HID adapters and other board variants need more work.
Tier-2 boards are implemented and unit-tested but have not been run against
commercial games; the FME-7's 5B expansion audio is not implemented.

## Layout

```
site/       the website at emulia.website, and the privacy policy Play requires
core/       nes-core: no I/O, threads or frame-time allocation
runner/     nes-runner: traces, frame hashes and automated ROM tests
desktop/    nes-desktop: macOS/Linux SDL2 + OpenGL + egui shell: shelf, saves, time control, looks, gamepads
native/     nes-android: JNI boundary and AAudio output
android/    Compose shelf, play view, picture and control settings, save UI
scripts/    native builds and external ROM regression runner
```

## Build and test

```sh
cargo test --workspace
cargo run -p nes-runner -- info <rom>
cargo run -p nes-runner -- trace <rom> [n] [--pc=C000]
cargo run -p nes-runner -- frames <rom> <n>
cargo run --release -p nes-runner -- rom-test <rom> [max-frames]

rustup target add aarch64-linux-android x86_64-linux-android
export JAVA_HOME="$HOME/.local/jdk"
export ANDROID_HOME="$HOME/Android/Sdk"
./android/gradlew -p android assembleDebug lintDebug testDebugUnitTest
./android/gradlew -p android :app:bundleRelease :app:assembleRelease
```

Gradle builds and packages Rust automatically. The release APK and AAB are the
shipping artifacts, signed with the upload key when `android/keystore.properties`
is present and with the debug key otherwise. `app-debug.apk` installs alongside
them as `com.bsteinfeld.emulia.debug` and carries x86_64 for emulators.
Outputs are under `android/app/build/outputs/`. Set
`-PbuildNumber=<integer> -PbuildLabel=<version>` to stamp a build by hand;
otherwise the version code is derived from the date so it always increases.

With a functioning emulator or connected tablet:

```sh
./android/gradlew -p android connectedDebugAndroidTest
```

The JNI unit tests run the real Rust bridge on the host JVM, and the presentation
geometry is unit-tested apart from the GL code that applies it. Instrumentation
tests exercise play/pause/save/load, settings persistence, cover precedence and
playtime; they require a stable Android device and have not yet passed in this
environment.

External test ROMs are not committed or shipped. See
[the test-ROM instructions](core/tests/roms/README.md). To run the broader suite:

```sh
python3 scripts/check-roms.py /path/to/nes-test-roms --report /tmp/accuracy.json
```

The GLSL lives in a Kotlin string, so the Kotlin compiler cannot see it and a
broken shader is a black screen on hardware. This compiles it without a device,
using `glslangValidator` from `PATH` or the Android SDK's emulator, and skips if
neither is present:

```sh
python3 scripts/check-shaders.py
```

Header corrections are generated from ROMs you have rather than shipped as
unverified data — see [the header database](docs/HEADER-DATABASE.md):

```sh
python3 scripts/build-header-db.py --self-test
python3 scripts/build-header-db.py --roms ~/roms --db corrections.tsv
```

The Android build needs Rust, a JDK 17 or newer, Android SDK 36 and NDK 28.2.13676358.
`scripts/build-native.sh` locates the NDK at run time from `ANDROID_NDK_HOME` or
`$ANDROID_HOME/ndk/`, targeting Android API 29, so no absolute path is committed.

## Releasing

Push a `v*` tag and GitHub Actions builds, tests, signs and publishes the APK and
AAB as release assets. [docs/RELEASING.md](docs/RELEASING.md) covers the signing
key, the repository secrets and what happens if this ever goes to Play.

## The website

[`site/`](site/) is <https://emulia.website> — static HTML with no build step. A
push to `main` touching it is mirrored by
[`publish-site.yml`](.github/workflows/publish-site.yml) into the public
`bsteinfeld/emulia-site`, which is what GitHub Pages serves; this repository stays
private. `site/privacy.html` is also the privacy policy URL Google Play requires.
See [site/README.md](site/README.md).

```sh
python3 scripts/check-site.py    # every local href and src resolves
```

To put a local build on the personal shelf instead:

```sh
JAVA_HOME="$HOME/.local/jdk" python3 scripts/publish-apks.py
```

The script verifies signing and 16 KB alignment, refuses anything carrying the
debug certificate, and copies the release APK and AAB with descriptive sidecars
into `.harness/artifacts/`. They appear on
[Controlplaine /stuff](https://bradley-desktop.tailc9ee0f.ts.net/stuff/).
Test APKs and downloaded ROMs are not published.
