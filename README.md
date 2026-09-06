# androidemu

A NES emulator for the OnePlus Pad 3, written from scratch in Rust with an Android
Compose shell. No ads, accounts, feature locks, bundled ROMs or network permission.

[PLAN.md](PLAN.md) is the full plan. [Phase 1 acceptance](docs/PHASE-1.md) records
what is implemented, tested and still needs device validation.

## Status

**Phase 1 preview builds are available; device acceptance is pending.**

| Component | Current implementation |
|---|---|
| CPU and bus | All opcodes, cycle-based accesses, interrupt polling, OAM and DMC DMA |
| Cartridges | iNES/NES 2.0, payload identity, mappers 0/1/2/3/4/7, battery RAM |
| PPU | Background/sprite pixels, scrolling, clipping, priority, sprite 0 hit, NTSC timing |
| APU | Five channels, nonlinear mixer, FIR anti-aliasing, 48 kHz samples |
| Persistence | Versioned deterministic states, validated transactional restore, SRAM |
| Android | Game shelf, ROM import, GLES video, AAudio, touch and two controller ports |
| Save UI | Ten slots, thumbnails, timestamps, overwrite confirmation, autosave and resume |

The 39-ROM CPU/APU/PPU/MMC3 regression set and the real `nestest` trace pass.
Device performance and latency are not yet measured. Sprite evaluation, some DMA
edge cases, PAL/Dendy, raw HID adapters and other board variants need more work.

## Layout

```
core/       nes-core: no I/O, threads or frame-time allocation
runner/     nes-runner: traces, frame hashes and automated ROM tests
native/     nes-android: JNI boundary and AAudio output
android/    Compose library, play view, controls and save UI
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
./android/gradlew -p android assembleDebug assemblePreview lintDebug testDebugUnitTest
```

Gradle builds and packages Rust automatically. `app-preview.apk` is an arm64
personal preview signed with the local debug key; `app-debug.apk` also contains
x86_64 for emulator testing. Outputs are under `android/app/build/outputs/apk/`.
Use the same signing key for subsequent updates to preserve Android app data.
Set `-PbuildNumber=<integer> -PbuildLabel=<version>` to stamp a published build.

With a functioning emulator or connected tablet:

```sh
./android/gradlew -p android connectedDebugAndroidTest
```

The JNI unit tests run the real Rust bridge on the host JVM. Instrumentation tests
exercise play/pause/save/load and lifecycle behavior; they require a stable Android
device and have not yet passed in this environment.

External test ROMs are not committed or shipped. See
[the test-ROM instructions](core/tests/roms/README.md). To run the broader suite:

```sh
python3 scripts/check-roms.py /path/to/nes-test-roms --report /tmp/accuracy.json
```

The machine uses Rust, JDK 21, Android SDK 36 and NDK 28.2.13676358. Linker paths in
`.cargo/config.toml` target Android API 29 and currently match Bradley's SDK path.

## Publishing personal previews

After the builds and checks pass, publish both installable APKs with:

```sh
JAVA_HOME="$HOME/.local/jdk" python3 scripts/publish-apks.py
```

The script verifies signing and 16 KB APK alignment, reads version metadata from
both APKs, and copies them with descriptive sidecars into `.harness/artifacts/`.
They appear on [Controlplaine /stuff](https://bradley-desktop.tailc9ee0f.ts.net/stuff/).
Test APKs and downloaded ROMs are not published.
