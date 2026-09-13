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

The initial Return/input, resize and Back to shelf test was manually accepted
by the user. A follow-up window capture confirmed the shelf in the same process.
The subsequent integration adds the existing pause menu and save-slot panel,
plus a Dolphin control-mapping editor. Menu/Escape takes one screenshot and
pauses the core; the native view is hidden while egui shows the menus over the
snapshot. Resume restores native rendering. There is no continuous readback.

Ten slots, replacement confirmations, thumbnails, screenshots, automatic saving
on pause/exit and automatic-save reopening now use the existing library paths.
The bridge has checked save/load operations, retaining Dolphin's state format.
Serialization and disk completion must succeed before a staging file replaces
the previous slot. Failed loads remain paused with a visible error.

The profile persists under `<data-dir>/dolphin/embedded-2606a` with a file lock.
It owns memory cards/NAND and initializes Metal with recording/analytics/counters
disabled. Only this experimental profile acknowledges NKit fixtures. Existing
Dolphin/temporary profiles are not migrated. Default builds and packages retain
the external launcher; embedded-mode shelf launches use the native host.

Player-one keyboard/SDL gamepad mappings are stored per game. An Emulia virtual
Dolphin device feeds the normal controller-expression engine, including analog
sticks/triggers and simulated Wii motion. GameCube, sideways Remote,
Remote + Nunchuk and Classic layouts are available; Wii can also choose a
GameCube controller for games that support it. Style changes require a restart;
individual bindings apply when resuming. Input is cleared for menus, focus loss
and disconnects. Additional players and rumble are not implemented.

Local Animal Crossing checks passed through `qualify.py`: save/load round trip,
PNG thumbnail, rejected invalid load, old-slot preservation on failed replacement,
GameCube A, proportional stick and trigger values, remapping A to Q, persistence
across reset, autosave and clean shutdown. An independent process reopen restored
the prior autosave. Captures also show the actual shared pause and slot panels.
These debug harness checks exercise the native bridge without macOS input
injection; physical controller feel still needs a manual check.

Mario Kart Wii passed the same state round trip/failure/thumbnail checks and
all three Wii native presets: sideways Remote, Nunchuk and Classic. The harness
verified their actual extension settings, A/C button expressions, proportional
IR pointer input, repeated restarts and clean shutdown. This qualifies the bridge
and configuration paths; it is not a race or motion-feel test for those presets.

Mouse aim uses AppKit coordinates inside the actual embedded view, with left
click A/right click B. After the user reported center-aligned but off-center
drift, the bridge was corrected to apply Dolphin's own `GetWindowInputScale()`
aspect-ratio adjustment, matching its Quartz mouse backend. Geometry tests cover
letterboxing, pillarboxing, Retina scaling and rejecting clicks in the bars. Further user feedback and
measurements on a copy of the saved Mario Kart Wii session showed an additional
in-game range mismatch: a half-width input moved the pointer approximately 276
pixels instead of 208.5 in the 834-pixel game image. Mouse-only multipliers of
0.75 horizontally and 0.96 vertically brought the sampled right/up/diagonal
positions within a few pixels relative to center. These are defaults for the
qualified USA disc ID `RMCE01`; other games retain Dolphin's default range.
Settings exposes independent per-game horizontal/vertical range overrides, and
existing overrides take precedence. Keyboard/gamepad aiming is unaffected. It follows resizing and fullscreen, ignores clicks on
Emulia's bars and menus, and requires held clicks to be released after a pause.
Keyboard/gamepad pointer movement takes over until the mouse moves again.
New Wii entries default to Remote + Nunchuk; existing per-game choices persist.
Unit tests cover mouse direction, bounds, click clearing and controller handoff;
manual mouse tracking/click feel still needs user verification.

The native bridge drains AppKit work before joining Dolphin during shutdown.
This fixes the observed boot-cancellation wait between Quartz device setup and
the main thread. The builder also refuses to overwrite an active probe app.
Dolphin remains loaded until process exit to keep its callbacks valid.

## Build and acceptance boundaries

[Build/run instructions](../scripts/dolphin-spike/README.md) pin the source and
supply a separate probe app and Cargo output. The local native build uses some
Homebrew libraries, including libraries built for macOS 26; it does not qualify
macOS 11 deployment or a self-contained distributable package. The upstream
source checkout, game images, profile data and screenshots are not committed.
Only the host bridge, build scripts and opt-in Emulia path are in this PR.

Feature-enabled macOS desktop checks: 148 tests pass (one optional commercial
SNES test ignored), 141 default-build tests pass, and strict Clippy passes. These retain the desktop regression
coverage; they do not exercise Dolphin's native renderer in unit tests. Rendering
was checked through the actual source-built bridge and screenshots. The build script completed with an isolated
Cargo output, a copied Sys resource directory, and successful ad-hoc bundle
signature verification.

Remaining acceptance work: physical gamepad/motion feel, longer embedded play,
and equivalent Linux/Android native surfaces. Production rollout also needs
portable native dependency packaging and an upstream update process. Android
embedding remains a subsequent prototype; the installed APK cannot donate its
surface to Emulia through the external-launch interface.
