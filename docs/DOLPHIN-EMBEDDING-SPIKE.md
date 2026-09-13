# Dolphin Android boot and macOS embedding spike

September 13, 2026. Authorized scope: test Android boot, then prototype rendering
Dolphin inside the existing PR #6 desktop shell. Rewind and fast-forward are not
requirements for this Dolphin experiment. Work stays on `multisystem/desktop-shell`
and PR #7 continues to target `feat/desktop-shell`.

## Android boot

Official Dolphin 2606a APK installed successfully on the dedicated
`emulia-multisystem-api35` Android 15/API 35 ARM64 tablet AVD. Package
`org.dolphinemu.dolphinemu`, versionCode 42830, primary ABI `arm64-v8a`.
The host is an Apple M2 Max; emulator acceleration check reports
Hypervisor.Framework available. No physical tablet was used.

APK source: https://dl.dolphin-emu.org/releases/2606a/dolphin-2606a.apk

Downloaded SHA-256:
`cc174dada5577ef045b3cd4c7c9801a2830cf68ac5495f083ecfe31c231e8fb4`.

The user's Animal Crossing GAFE01 NKit ISO was copied only into the test AVD's
Downloads directory, then selected using Dolphin's Open File action and
Android's document picker. First-run analytics was declined. The installed
Dolphin app rendered animated scenes, the title screen and its touch overlay.
A held touch on Start advanced to K.K.'s initial conversation. A Home/background
and return to the existing task also returned to a rendered game.

| AVD graphics mode | Observations |
|---|---|
| `swiftshader_indirect` | Title/animation rendered; captured ~16% speed and ~10 FPS/VPS |
| `host` | Android Emulator OpenGL ES Translator (Apple M2 Max), GLES 3.0; title captures showed 35% then 79%; the opening conversation showed 100%, 59.95 FPS and 59.94 VPS |

Both modes displayed Dolphin's missing `EXT_buffer_storage` warning. These are
individual overlay readings across different scenes and host loads, not an
average, controlled comparison or sustained-play benchmark. Native/Cargo builds
and a macOS probe were also running during parts of the test. The emulator was
started with `-no-audio`; audio fidelity/latency is unqualified and the guest
reported audio HAL write errors. No tablet-performance prediction follows from
these runs. They establish that the release APK boots, renders and accepts a
touch input on the AVD.

Storage matters: a shell attempt to grant a fabricated external-storage URI
failed with SecurityException, and an ungranted URI failed to open. Using the
actual document picker succeeded. This is not validation of an Emulia-to-Dolphin
Android URI launcher; no such launcher or embedded Android view has been added
to the current branch. The previous 16 Android instrumentation tests remain
historical NES/UI results, separate from this Dolphin boot probe.

## macOS embedded rendering

Dolphin tag 2606a, commit `c77bbaa0f372c3f72281602a8b087206706542cb`, was
built from source with a small Objective-C++ host bridge. A Cargo feature loads
that bridge into the existing `nes-desktop` process. The bridge creates a native
child view between Emulia's top and bottom bars and gives it to Dolphin's Metal
renderer. SDL/egui continues drawing the existing shell with OpenGL. There is
one Emulia window, no external Dolphin child process for this game, no window
capture, and no framebuffer readback/upload loop.

Animal Crossing's animated title rendered inside that window. A screenshot
showed 100% speed, 59.87 FPS and 59.86 VPS. Native diagnostics identify JITARM64,
Metal and HLE. This qualifies the rendering approach for this initial macOS
arm64 fixture, not all games or platforms.

The host supplies start/pump/stop calls. Escape and Back to shelf stop Dolphin,
shut down its core/controllers, then remove its native view. Resizing adjusts
the view and requests a Dolphin surface resize. The user reported that the requested Return/input, resize and Back to shelf
check looked good. A follow-up window capture confirmed the Emulia shelf was
visible again in the same process, with the embedded rendering view removed.
This is one manual smoke check; repeated sessions and failure recovery remain
unqualified. macOS input automation is unavailable.

The prototype builds only when explicitly requested; default application and
packaging behavior retain the external Dolphin launcher. A fresh temporary
profile isolates each run. It deliberately enables Metal/counters and disables
recording/analytics, with NKit warnings acknowledged only in that probe profile.
Rewind, fast-forward and Emulia save slots are absent. Dolphin is retained in
memory until process exit to avoid unloading code referenced by its globals.

## Build and acceptance boundaries

[Build/run instructions](../scripts/dolphin-spike/README.md) pin the source and
supply a separate probe app and Cargo output. The local native build uses some
Homebrew libraries, including libraries built for macOS 26; it does not qualify
macOS 11 deployment or a self-contained distributable package. The upstream
source checkout, game images, profile data and screenshots are not committed.
Only the host bridge, build scripts and opt-in Emulia path are in this PR.

Default and feature-enabled macOS desktop checks: all 140 tests pass (one optional commercial
SNES test ignored), and strict Clippy passes. These retain the desktop regression
coverage; they do not exercise Dolphin's native renderer in unit tests. Rendering
was checked through the actual source-built bridge and screenshots. The build script completed with an isolated
Cargo output, a copied Sys resource directory, and successful ad-hoc bundle
signature verification.

Next acceptance work: extended embedded input/gamepad mapping, window close and
repeat-launch lifecycle, saved-progress reopening, longer play, and equivalent
Linux/Android native surfaces. Production integration also needs deliberate
profile ownership, user-visible error handling, build/dependency packaging and
an upstream update process. Android embedding remains a subsequent prototype;
the installed APK cannot donate its surface to Emulia through the current
external-launch interface.
