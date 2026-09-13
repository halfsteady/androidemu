# Emulia multisystem implementation handoff

## Current branch and authoritative foundation

Work now lives in `/Users/freelove/androidemu` on **`multisystem/desktop-shell`**,
branched directly from PR #6's `feat/desktop-shell` at
`e153af1549633bd03960d1eff4b583bbdf60df7e`. PR #6 is the integration base, not merely
a visual reference. Target its branch when preparing the integration PR.
The separate `.claude/worktrees/desktop-shell` checkout remains on the original
`feat/desktop-shell` branch.

Preserve PR #6's existing NES engine/session, Rust/egui GUI, settings, input and
persistence behavior. Add jgenesis as a second core through that existing shell.
Do not replace the shell with the earlier Compose desktop frontend. Shared
abstractions should serve necessary integration changes rather than force a rewrite
of the already working NES path.

## September 13: native desktop integration

PR #7 targets `feat/desktop-shell` (PR #6), not main. The recovered API/runtime
and adapters are now members of the Cargo workspace. The original NES engine
is unchanged, moved to `desktop/src/engine/nes.rs`; an enum dispatches to it or
the jgenesis SNES engine. No NES hardware code was changed.

Implemented in the existing shell: SNES import with canonical copier-header
identity; separate game.sfc storage; native frame dimensions/aspect; stereo audio;
existing slots, thumbnails, screenshots, battery recovery and autosaves; bounded
rewind; silent SNES fast-forward; separate 12-button keyboard/gamepad profiles
through the existing wizard. SNES shoulders are gameplay inputs, triggers control
time. NES palettes/overscan settings remain NES-specific.

Native packaging replaces the earlier Compose packaging: macOS app/DMG,
Debian and RPM, brand icon, X11/Wayland application ID, desktop entry, AppStream
metadata, source archives and license notices. Build commands are in
[DESKTOP-PACKAGING.md](DESKTOP-PACKAGING.md). Generated packages live under
`target/packages/`; older ignored `desktop-ui/build` artifacts are obsolete.

Verified locally: original 130-test desktop baseline; 134 desktop tests pass on
macOS and Linux (the commercial-ROM test is separately run);
strict desktop Clippy; shared runtime and adapter tests; Super Mario World
first-level route, exact state replay, rewind and autosave reopening on macOS
arm64 and Debian 12 arm64; packaged NES/SNES executable smoke checks on macOS arm64 and Debian 12
arm64; Debian installation, metadata, actual window/pause/settings rendering
under Xvfb/Mesa, WM_CLASS and icon; macOS bundle signature and DMG integrity.
RPM builds, but installation on an RPM distribution is not qualified. CI covers
Linux/macOS arm64 and x64. Remote jobs on this update did not start: GitHub
reports failed account payments or an insufficient spending limit. See run
`34765764588`. The account owner must resolve Billing & plans before remote
validation can run. PR #7 remains open and has not been merged into PR #6.

See [SNES-SMW-QUALIFICATION.md](SNES-SMW-QUALIFICATION.md) for exact evidence and
limits. Manual audio/input assessment, real in-game SRAM checkpoint, extended
compatibility and physical desktop sessions remain acceptance work. Android
integration from the earlier checkpoint has not yet been restored onto this
branch; Android itself remains the PR #6 version. Physical tablet validation
is deferred, not a blocker. The initial desktop Dolphin launcher is implemented below.
Do not resurrect the shared Compose desktop: the user's explicit PR #6
foundation decision supersedes that part of the original feasibility plan.

All previous uncommitted work, including the interrupted Compose UI edits, is
preserved in stash commit `d59231721ac68868d82dd2a10fb92a2ab6f87b26`, also retained by
`refs/emulia/checkpoints/pre-pr6`. Recover selected tracked files from that commit
and formerly untracked files from its third parent. Do not apply the whole stash
over PR #6: that would replace its desktop shell. The older branch is
`multisystem/nes-core-boundary`.

## September 13: external desktop Dolphin launcher

The existing shell now imports GameCube/Wii ISO/GCM references, streams their
fingerprints without copying discs, and supports relinking moved files. Settings
selects Dolphin, with macOS app/PATH discovery as defaults. Launch uses a literal
argument array `--batch --exec <absolute path>` and Dolphin's existing user
profile. Emulia saves/closes any embedded session before handoff, drains bounded
stderr diagnostics, prevents concurrent managed launches, and raises its shelf
when the child exits. Dolphin owns controls, settings and saves; no speed or
recording overrides are applied. Closing Emulia leaves Dolphin running.

Local macOS checks: 138 desktop tests pass, one commercial-ROM test ignored;
strict desktop Clippy passes. Updated macOS app/DMG builds, packaged NES/SNES
smoke checks, bundle signature and DMG integrity checks pass. Existing Linux
package artifacts predate the Dolphin launcher and need rebuilding. Tests cover reference identity/relink/deletion,
literal paths, subprocess failures and large stderr output, settings recovery,
and actual shell handoff from NES through subprocess completion. Dolphin 2606a
on macOS arm64 rendered the user's Animal Crossing GAFE01 title screen. The user
confirmed normal speed after diagnostic PNG frame dumping was disabled. The
`.iso` is an NKit image; production launches retain Dolphin's warning. See
[DOLPHIN-QUALIFICATION.md](DOLPHIN-QUALIFICATION.md) for exact limits. Real GUI
return focus, controller/memory-card workflows, Linux Dolphin and Wii gameplay
remain unqualified; this is not completion of the full compatibility matrix.
The account billing blocker still prevents remote CI from starting.

## Historical implementation record (superseded where it conflicts above)


The research report is [MULTISYSTEM-FEASIBILITY.md](MULTISYSTEM-FEASIBILITY.md). It assesses separate SNES emulation, a shared Android/desktop Compose GUI, and external Dolphin launching. All three are feasible within the boundaries described there. Research changed no application code; the first implementation milestone is now recorded below.

**Paste this into the next implementation chat:**

```text
Read docs/MULTISYSTEM-FEASIBILITY.md and docs/MULTISYSTEM-HANDOFF.md, including
its implementation record. Work is on multisystem/nes-core-boundary, based on
main at d3d0b351d9736ee74746074fdffcebb800d8deef. Inspect HEAD and uncommitted
changes before editing; do not recreate the completed API/adapter milestone.

The user authorized proceeding through milestones until blocked. Phase 1/2 media,
shared history/scheduling, versioned library migration, and the shared Compose
shelf/theme/model extraction are implemented. A macOS desktop app image and initial
in-window player exist; desktop milestone 5 is NOT complete. Read the latest
implementation record and docs/DESKTOP-COMPOSE.md before editing.

Desktop gamepad, ten save slots with thumbnails, aspect/trim controls, and fixed
rewind jumps are implemented. Continue PC-first on macOS and Linux; Windows is
explicitly excluded. Android emulator checks are the current acceptance gate.
Physical-tablet testing is deferred to the user’s coworker and is NOT a blocker.
Keep
all working-tree changes; nothing has been committed or pushed.
Preserve nes-core hardware, Android identity, JNI compatibility, NES IDs and saves,
and SDL behavior. Keep Android/macOS/Linux as the initial targets. SNES remains
an independent core; the user selected jgenesis with GPL-compatible distribution
on September 13, 2026. See the latest implementation record. Rewind, fast-forward, and save states are mandatory for SNES on
Android and the eventual shared desktop GUI. Runtime owns history and scheduling;
serialization alone does not qualify a core. The user has authorized later implementation milestones.
Do not ask again about the accepted jgenesis/GPL decision.
Dolphin remains a later external launcher with its own gameplay and saves.
```

**First milestone: scope and acceptance.** Add an `emulation-api` crate containing system/core identity, capabilities, input and output descriptors, and a minimal embedded session contract. Add a NES adapter that owns `nes_core::Nes`. Use workspace naming consistent with current conventions. Put shared runtime orchestration in a separate crate only where there is actual common behavior to extract; an empty framework is not a deliverable.

Keep the API small. It needs load/create, frame stepping, current video access, audio draining or delivery, input, reset, and explicit persistence operations. Represent errors and unsupported operations. Descriptors should represent width, height, stride, pixel format, visible area/aspect, audio rate/channels, and timing. Stable serialized numeric identifiers need explicit mappings, not accidental enum ordinals. Decide which side owns each buffer and how long borrowed data remains valid.

The NES adapter must preserve palette presentation, cartridge correction notes, original ROM identity, battery-save behavior, and existing `ANES` states. Do not replace these with a new hash or generic state serializer. Avoid per-pixel virtual dispatch; perform adaptation at frame boundaries. Do not allow raw `Nes` internals to become the shared contract. A narrow NES-specific adapter extension is preferable to exposing the bus to every frontend.

It is acceptable for the old JNI function names to remain compatibility wrappers. Native storage can initially hold a single adapter session. Preserve serialized access and buffer-capacity checks. Keep AAudio platform-specific. In SDL, preserve current command-line behavior and save locations; ensure audio/pacing calculations still operate in the expected units. If full JNI/SDL routing requires a larger change, finish and verify one vertical slice, record the remaining exact call sites, and avoid claiming phase 1 complete.

Acceptance requires existing NES tests to retain their results and meaningful adapter equivalence tests. Run the same generated or already-available ROM through direct `Nes` and the adapter using a fixed input sequence; compare output and persisted states. Test reset retaining battery data, restore followed by replay, and error behavior. Validate rendered output against the current palette expansion rather than comparing incompatible indexed and RGBA buffers. Integration tests should verify actual behavior, not just the presence of trait methods.

**Files to inspect first:**

| File | Purpose |
|---|---|
| `Cargo.toml`, `core/Cargo.toml` | Workspace and dependency-free NES core |
| `core/src/nes.rs`, `core/src/lib.rs` | Public NES operations and exported types |
| `core/src/state.rs`, `core/src/rewind.rs` | Legacy persistence and rewind assumptions |
| `native/src/lib.rs`, `native/src/audio.rs` | JNI ownership, palette expansion, mono AAudio |
| `desktop/src/main.rs`, `desktop/src/palette.rs` | Existing SDL player, pacing, and persistence |
| `android/app/src/main/java/dev/androidemu/Native.kt` | Current JNI signatures |
| `android/app/src/main/java/dev/androidemu/GameSurface.kt` | Frame scheduling and native command threading |
| `android/app/src/main/java/dev/androidemu/Library.kt` | Legacy paths, IDs, index records and import limit |
| `android/app/src/main/java/dev/androidemu/MainActivity.kt` | Application orchestration and UI coupling |
| `android/app/build.gradle.kts`, `scripts/build-native.sh` | JNI build inputs, ABI packaging, host tests |
| `.github/workflows/`, `scripts/check-desktop.py` | Existing verification and release contracts |

**Verification commands.** Use the repository's current documented environment. These commands are starting points, not claims that the tools or external ROM fixtures are installed. Add new crates to relevant build inputs and CI so changing an adapter rebuilds the native libraries.

```sh
cargo test --locked -p nes-core -p nes-runner
cargo test --workspace --locked
cargo build --release --locked -p nes-desktop
python3 scripts/check-desktop.py target/release/nes-desktop --require-native-drivers
python3 scripts/check-shaders.py
./android/gradlew -p android assembleDebug lintDebug testDebugUnitTest
```

After introducing workspace members, update the lockfile intentionally if Cargo requires it; use `--locked` again for verification. External accuracy ROMs require local fixtures as documented in `core/tests/roms/README.md`. Run instrumentation only when a usable device is available. The Gradle host-native test task currently assumes a Linux `.so`; fix OS-specific naming if it blocks macOS JNI verification, and report that separately from emulator correctness.

**Follow-on sequence.** Each row is a reviewable milestone, not one enormous refactor.

| Order | Work | Done when |
|---|---|---|
| 2 | Add synthetic second-core fixture; generalize video, stereo, logical input and capability UI | Changing geometry and stereo channels render/play correctly; unsupported controls are absent |
| 3 | Version library schema and add content locators | Legacy NES rows and files survive migration, interruption, reload, and archive/unarchive |
| 4 | Extract shared Compose theme, shelf, models and platform interfaces | Android and desktop display the same shared shelf implementation |
| 5 | Integrate desktop play view, audio, controller input, saves and packaging | Shared desktop GUI is usable in-window on macOS/Linux with packaged native libraries |
| 6 | Compare upstream integration with original-core development before selection | Decision record compares scope, licensing, maintenance, upstream spike results, and original-core milestones |
| 7 | Finish SNES import, input, persistence, compatibility matrix and required rewind/fast-forward/save states | All three workflows and their interactions pass on Android/desktop for the declared compatibility set without NES regressions |
| 8 | Add external desktop Dolphin launcher | Selected executable, arguments, diagnostics and return behavior pass |
| 9 | Add Android Dolphin activity/URI launcher | Selected release APK, storage providers and lifecycle cases pass on tablet |

Move the small SNES build/performance spike or Dolphin Android launch spike earlier if their assumptions would affect implementation choices. The full integration still follows the required core, storage, and UI foundations.

**Decisions to carry forward.** The confirmed preference is to compare existing-core integration with writing an original SNES core before deciding. Architecture work is independent of that selection. Compare both against the same initial games/features, distribution policy, maintenance expectations, and delivery priorities. Use bounded upstream spikes and an original-core milestone/test plan as evidence; a small original-core prototype cannot establish broad compatibility. Do not silently choose an implementation for distribution. jgenesis is a natural Rust candidate but has not been Android-validated here. Snes9x has a libretro integration route and noncommercial terms. bsnes is another GPL candidate. A new original SNES core is a separate many-month project. Assume Android, macOS, and Linux first; add Windows only with its own accepted build and interactive verification matrix.

**Mandatory embedded time controls.** Neither SNES path qualifies without rewind, fast-forward, and save states. Preserve manual slots/autosaves/thumbnails, variable-speed scrubbing, and 5/15-second rewind jumps when history permits. The current control requests up to eight frames per displayed frame; benchmark achieved SNES speed with rewind capture enabled. Preserve fast-forward input, clear input entering rewind, mute output during time controls, hold at history exhaustion, and resume without stale audio. Maintain history through fast-forward. Validate snapshot completeness for supported chips, restore/replay determinism, failed-restore isolation, total memory including temporary snapshots, and branching after rewind. Account for variable-length serialization: the current delta chain clears on length changes. Add these gates to the core comparison and re-estimate integration after the spike. Capability flags still describe other backends, but hiding these controls does not satisfy SNES acceptance.

**Dolphin constraints that must survive future chats.** Emulia supplies the library and launch experience; Dolphin owns gameplay, controls, settings, and saves. Desktop launch uses an argument array, initially `--batch --exec <absolute path>`. Android launch targets a validated exported main activity with readable URI data/ClipData and read grants. Its private emulation activity is not a launch target. Detect installation using narrow package visibility declarations. Test the actual installed version; current upstream source is not a compatibility guarantee for all APKs.

GameCube/Wii files must not use the 16 MiB in-memory NES importer. Model files/URIs and ordered disc sets. Returning to Emulia is not proof that Dolphin stopped, and a simple launch does not expose rewind, pause, screenshots, or save-state control to Emulia. Preserve this distinction in capabilities and playtime accounting.

**Implementation record — September 12, 2026.** Phase 0's host baseline is recorded and the bounded phase 1 implementation is complete on `multisystem/nes-core-boundary`, created from the user's current `main` at `d3d0b351d9736ee74746074fdffcebb800d8deef`. The two research documents were already untracked and were preserved. Android build/device acceptance remains outstanding; no release or cross-platform performance claim is made.

The new [`emulation-api`](../emulation-api/src/lib.rs) defines a static load factory and object-safe embedded contract, system/core/content identity, capabilities, explicit logical button bits, checked RGBA geometry, visible area/pixel aspect, presentation mode, audio rate/channels, timing, and fallible operations. Storage uses explicit string IDs rather than enum ordinals. Video/audio/battery buffers are borrowed from the adapter; hosts copy before queuing across threads. Audio represents the most recently completed frame and is consumed once by the host. The host owns resampling and pacing. No dynamic ABI or external-launcher implementation was added.

[`NesAdapter`](../cores/nes-adapter/src/lib.rs) owns `Nes` privately and caches RGBA output. Palette changes and state restoration repaint without stepping. Persistence retains the existing ANES writer and all legacy readers; an adapter flag suppresses stale restored/reset audio without changing serialized machine bytes. NES-specific extensions expose palette selection, header correction notes, existing content IDs, fresh power cycling, and an infallible legacy state writer. `core/` hardware, state codecs, and `runner/` remain unchanged.

Both [`native/src/lib.rs`](../native/src/lib.rs) and [`desktop/src/main.rs`](../desktop/src/main.rs) now use the adapter for every machine operation. Neither accesses a NES bus/CPU. Android retains its JNI symbols, direct-buffer checks, serialized GL-thread calls and mono AAudio. SDL retains RGB24 presentation, square source pixels, controls, pacing and save locations; its audio queue calculation explicitly includes channel count. The default palette is centralized in the adapter, with the original table retained as a test fixture.

Two compatibility differences were deliberately preserved: Android uses `header.hash` for library IDs while SDL uses `header.identity` (which includes trailing original payload bytes); Android reset creates a fresh machine retaining battery RAM, while SDL reset invokes the hardware reset sequence. No library/save migration occurred.

Gradle now tracks the API/adapter sources and all relevant manifests for Android and host JNI rebuilds, and uses `System.mapLibraryName` for host output tracking. Desktop CI watches and tests the new crates. Cargo.lock adds only local workspace packages/edges; no third-party emulator or other dependency version was changed.

| Verification | Observed result on macOS arm64 |
|---|---|
| Baseline `cargo test --locked -p nes-core -p nes-runner` | Cargo reported 143 passing tests; nestest internally skipped because its external fixtures were absent |
| Final `cargo test --workspace --locked` | Cargo reported 151 passing tests: same baseline plus 5 adapter equivalence/validation tests and 3 SDL persistence tests; nestest still skipped |
| `cargo clippy --workspace --locked --all-targets --no-deps -- -D warnings` | Passed |
| `cargo build --release --locked -p nes-desktop -p nes-android` | Passed |
| `python3 scripts/check-desktop.py target/release/nes-desktop --require-native-drivers` | Passed video/audio, muted pacing, SRAM, autosave recovery and invalid inputs; compiled Cocoa/CoreAudio drivers present; smoke execution is not interactive gameplay validation |
| Kotlin/JNI `NativeBridgeTest` against the host library | All 7 tests passed, including rewind, failed operation isolation, reset, state replay, and new palette repaint/load/restore/reset coverage |
| Gradle `:app:buildHostNative` with Java 17 | Passed on macOS; second invocation correctly UP-TO-DATE with `.dylib` output |
| `python3 scripts/check-shaders.py` | All 4 shaders passed with `DYLD_LIBRARY_PATH=/opt/homebrew/opt/glslang/lib`; the unmodified script copies Homebrew's validator and otherwise loses its relative dylib lookup |
| `assembleDebug lintDebug testDebugUnitTest` | Default Java 25 failed with `25.0.2`; retried with temporary Java 17, which reached task dependency resolution and failed because Android SDK location was absent |

For host JNI verification without an Android SDK, the actual `Native.kt` and `NativeBridgeTest.kt` were compiled using Gradle 8.11.1's bundled Kotlin 2.0.20 compiler and JUnit 4.13.2, then run with `org.junit.runner.JUnitCore` and `java.library.path=target/release`. A temporary Temurin 17 runtime was downloaded under `/tmp/emulia-jdk17`; no system Java installation or repository toolchain setting was changed. This validates the Kotlin/JNI boundary, not the configured Android Kotlin 2.1.21 build or the entire Android unit suite.

The adapter tests run the same generated ROM and fixed two-port inputs through direct `Nes` and `dyn EmbeddedCore`, comparing exact states, expanded legacy palette pixels, non-silent audio, and battery bytes for NTSC/PAL. They also cover restore/replay, both reset forms, invalid operations leaving every output intact, actual v1/v2 fixtures including a rendering snapshot, repaired-header notes, differing legacy IDs with trailing payload, and descriptor overflow/crop/stride rejection. No external accuracy ROM suite was downloaded or claimed as passing.

**Precise remaining boundaries and next task.** JNI still assumes a 256×240 direct buffer in `blit`, `frame`, `repaint`, and `rewind`. SDL allocates one texture at session start and still converts RGBA to RGB24. AAudio remains mono. `GameSurface.onDrawFrame`/`skipBack` own batching and pacing, while JNI's `REWIND` owns history using `nes_core::rewind::Rewind`. This is the only remaining direct native dependency on `nes-core`; no empty runtime crate was introduced. Keep the current ownership until a tested extraction moves real common orchestration into `emulation-runtime`.

Next, use a synthetic second core to test descriptor changes, stereo and unsupported controls before connecting a real SNES core. This must drive safe texture/buffer resizing and audio-frame accounting, then shared runtime scheduling/history extraction. Preserve the mandatory SNES time-control gates above; this milestone has not qualified a SNES implementation or solved variable-length rewind snapshots. Library schema migration and shared Compose UI remain subsequent milestones.

Before Android acceptance, configure Java 17 and the documented SDK/NDK/Rust Android targets, run the full Gradle checks, and validate actual device audio, lifecycle, time controls and native alignment. Linux builds/interactive packaging, extended gameplay performance, and external accuracy suites were not run here. SNES core selection, dependency distribution policy, Windows scope, and initial SNES compatibility/peripherals remain open; architecture work has made no licensing decision.


**Phase 2 implementation record — September 12, 2026.** Continued automatically on the same branch at the user's request. The media and capability implementation is present and host-verified; Android UI/device acceptance is still open. The synthetic fixture is intentionally not a SNES implementation or a product-selectable backend.

- Added [`synthetic-core`](../cores/synthetic/src/lib.rs), a generated diagnostic core that cycles through 256×224, padded 512×478 interlaced, and padded 320×240 buffers, with nonzero visible origins, pixel aspect metadata, distinct stereo channel markers, one input port and explicit unsupported-state/battery cases. Normal JNI and SDL builds do not expose a synthetic loader.
- Added actual shared media behavior in [`emulation-runtime`](../emulation-runtime/src/lib.rs): bounded video validation, checked row copies that omit padding, mono/stereo format validation, audio-frame/queued-byte accounting, and whole-frame stereo interpolation. Tests verify output pixels, padding, changing dimensions, channel separation, underrun/reset behavior and exact equivalence with the former mono interpolation sequence. Scheduling/history have not yet moved into this crate.
- SDL's `play` now accepts `dyn EmbeddedCore`, recreates RGBA textures on size changes, respects visible rectangles and pixel aspect, masks unsupported inputs and guards save hotkeys. NES explicitly retains its existing square-pixel SDL presentation. The existing SDL integration test also runs nine synthetic frames through the real video/audio loop.
- JNI now provides `step`, `videoDescriptor`, `copyVideo`, `systemId` and `capabilities`. Stepping is separate from copying, so a short buffer can be retried without advancing the core. Old JNI symbols and NES byte-input semantics remain compatible. The static machine enum has a synthetic variant only under the opt-in `diagnostic` Cargo feature; unsupported legacy frame calls are rejected before stepping. Diagnostic libraries are built in temporary directories by the test script, not into Gradle's packaging directory.
- Android's `GameSurface` uses the new step/query/copy path. `VideoInfo` validates metadata before direct-buffer allocation. Renderer source textures, smoothing/composite resources, crop coordinates, shader uniforms and preview readback accept changing dimensions. Screenshots/thumbnails use the actual frame size. The generic layout calculation avoids a floating-point round-trip that lost one pixel on height-constrained surfaces. NES hardware aspect is now correctly described as 8:7 in adapter metadata; the old SDL square-pixel choice remains a host override.
- Android AAudio now queues complete mono/stereo frames and shares one interpolation phase across channels. Sample-rate/channel configuration occurs before stream start; queue statistics remain measured in audio frames. The existing mono interpolation behavior has a direct comparison test. Rust arm64 compilation checks this platform-specific module, but actual device sound/latency is unverified.
- Capability models guard unsupported save/time controls, gamepad buttons and NES-only palette/trim options. Save/restore orchestration respects persistence capabilities. Non-NES shoulder buttons remain gameplay inputs; system-specific controller mappings preserve the legacy NES preference keys. These flags do not relax the mandatory rewind/fast-forward/save-state requirements for a future shipping SNES core.
- [`scripts/check-host-jni.py`](../scripts/check-host-jni.py) builds a temporary diagnostic JNI library and runs real Kotlin/JNI plus geometry tests without requiring an Android SDK. Desktop CI now runs this check with Java 17, and watches the new crates/native/Kotlin paths. Gradle native inputs include the runtime and diagnostic crate manifests/sources.

| Phase 2 verification | Result |
|---|---|
| `cargo test --workspace --locked --all-features` | 156 tests reported passed, including 5 new runtime tests; the external nestest fixture still internally skips |
| `cargo clippy --workspace --locked --all-targets --all-features --no-deps -- -D warnings` | Passed |
| `PATH="$HOME/.cargo/bin:$PATH" cargo clippy -p nes-android --locked --target aarch64-linux-android --no-deps -- -D warnings` | Passed, including AAudio; installed the Rust Android target through rustup. Homebrew's separate rustc does not use rustup's target libraries, so put rustup first for this check |
| `JAVA_HOME=<JDK17> python3 scripts/check-host-jni.py` | 17 tests passed: 8 existing/new NES JNI, 6 existing geometry, 2 generic metadata/geometry, 1 synthetic JNI integration |
| Release desktop/native build and `scripts/check-desktop.py ... --require-native-drivers` | Passed; NES video/audio/persistence smoke checks remain green |
| Shader validation with the Homebrew dylib-path workaround above | All 4 shaders passed after generic crop-uniform changes |
| Full Gradle assemble/lint/unit checks with Java 17 | Passed after authorized SDK installation; see Android verification below |

**Next implementation boundary.** SDK installation and license acceptance were explicitly authorized by the user. Android build and emulator verification are recorded below. The app must remain NES-only until library/schema/import work and real-core qualification land. Device stereo output and performance still require physical hardware.

Rewind still uses the existing NES byte-delta history in JNI, and `GameSurface` still owns speed batching. Variable-size snapshots, total history memory accounting, common scheduling, library migration, shared Compose desktop UI, and all real SNES/Dolphin integration remain follow-on work. No upstream SNES dependency or distribution-license choice has occurred. Android tooling was installed under the current user’s home directory.


**Android verification follow-up — September 12, 2026.** The user authorized SDK/NDK installation and license acceptance. Installed Android tooling under `/Users/freelove/Android/Sdk`; ignored `android/local.properties` points there. Installed platform 36, build-tools 35.0.0, NDK 28.2.13676358, platform-tools, native Apple Silicon command-line tools/emulator, and an API 35 Google APIs ARM64 image. Java 17 is at `/tmp/emulia-jdk17/jdk-17.0.20.1+1/Contents/Home` and is temporary; provision another Java 17 installation if that directory is removed. Put `/Users/freelove/.cargo/bin` first in PATH so the installed Rust Android targets are used.

Reproduction from the repository root:

```sh
JAVA_HOME=/tmp/emulia-jdk17/jdk-17.0.20.1+1/Contents/Home \
ANDROID_HOME=/Users/freelove/Android/Sdk PATH="$HOME/.cargo/bin:$PATH" \
./android/gradlew -p android assembleDebug assembleDebugAndroidTest lintDebug testDebugUnitTest --console=plain
```

The complete app and instrumentation APK compile; all 37 JVM tests pass. Lint reports no errors (27 warnings and one informational finding remain). Both packaged native ABIs have 16 KiB ELF LOAD alignment, and `zipalign -c -P 16 4` passes for the debug APK.

Added actual GLES regression tests in `RendererTest`: repeated source growth/shrink, visible-rectangle cropping, every picture filter, preview framebuffer preservation, and recreation after EGL context loss. The latter reproduced GL_INVALID_FRAMEBUFFER_OPERATION before a fix: `ScreenRenderer.create()` must invalidate cached preview framebuffer handles and readback storage when a new context is created. The tests now cover that recovery directly.

Existing `PlayFlowTest` fixtures now remove only their generated ROM entries before each test so reused content IDs do not retain a previous test's title/autosave. The settings test works with an empty shelf and scrolls the vertical panel to the note below the filter row before scrolling horizontally to the desired chip; merely scrolling a chip targets its horizontal parent and can leave the entire row outside the panel viewport.

The full instrumentation suite passed **14 tests** (12 play-flow and 2 GLES tests), including save/load, reset, rewind, settings persistence, and lifecycle coverage. The dedicated `emulia-multisystem-api35` AVD uses API 35 ARM64, a tablet display, and SwiftShader GLES. This verifies Android UI/lifecycle and GLES behavior in an emulator; it does not qualify physical tablet GPU performance, audible stereo output, latency, or sustained fast-forward speed. Physical-device acceptance and the later shared scheduling/history milestone remain open. No real SNES core has been integrated.


**Shared runtime, library and Compose follow-up — September 12, 2026.** Continued after the user explicitly requested proceeding through milestones until blocked.

- `emulation-runtime::history::Rewind` now owns Android history. Equal-length states use the existing XOR/run encoding; size changes retain full predecessor snapshots instead of clearing history. Restore commits a history pop only after the core accepts the candidate. The memory budget conservatively reserves anchors, caller snapshots, encoding/restore scratch and container allocation; allocator-internal bookkeeping and arbitrary allocations performed inside a core are outside its control. Oversized snapshots disable history rather than exceeding the accepted retained-state budget. Tests cover size changes, branching, exhaustion, failed restoration and budget eviction.
- `emulation-runtime::scheduling` owns frame-clock pacing and forward/rewind batching. Android's `GameSurface` now requests batches and copies video separately, including rewind jumps. JNI no longer depends directly on `nes-core`; hardware and its existing rewind tests remain untouched. Actual NES tests compare eight-frame fast-forward with direct execution, retain history through the batch, hold at exhaustion, and resume audio delivery. Android stops/flushed AAudio for time controls; UI still owns gestures, vsync notifications and platform stream lifecycle.
- Library schema 1 wraps rows in `{version,games}` and stores explicit system strings plus ordered owned-file/document-URI/external-file locators. Legacy arrays default to NES and `game.nes`; reading alone does not rewrite them. Mutations use the platform atomic writer, unknown versions are rejected before writes, and current NES imports/save paths stay unchanged. Instrumentation tests verify migration, interrupted-write recovery, archive/unarchive, unchanged save/art bytes and ordered external discs without routing them through the NES byte importer.
- `shared-ui/src/main/kotlin` is compiled into both Android and desktop. It contains the common shelf layout, theme/widgets, game/content models, JSON codec, platform shelf interface and JNI declarations. Platform card content/file pickers/storage and gameplay rendering stay in their hosts. This is shared JVM-compatible Compose source, not an iOS/web migration. Android's activity now invokes the shared shelf.
- `desktop-ui` adds a Compose macOS/Linux entry point, real NES import/archive, legacy SDL IDs, an initial in-window keyboard player, Java Sound audio, time controls and interoperable SDL battery/manual-state paths. The host library is included in the app resources with a bundled Java runtime. See `DESKTOP-COMPOSE.md` for commands, exact paths and limitations. Desktop gamepad support and full picture/save UI are unfinished; do not label milestone 5 complete.
- Desktop CI watches the new shared/desktop paths and includes platform Compose tests and app-image creation. No push or remote CI run has occurred.

Verification so far: full Rust workspace reports 160 passing tests (external nestest fixtures still internally skip); strict Clippy passes for changed crates, while full workspace Clippy now reports `collapsible_match` in unchanged `core/src/apu.rs:451`. That unrelated hardware source was not edited. Host Kotlin/JNI reports 17 passing tests. Android assemble/lint and 37 JVM tests pass; the 16-test emulator suite includes library migration plus existing play/lifecycle/GLES coverage and passes after shared shelf extraction. The macOS packaged smoke command verifies bundled JNI loading and forward/rewind execution. Current desktop integration test results and remaining acceptance are recorded below.


Final local desktop checks: all **3 Compose desktop tests pass**, including an actual generated-ROM frame displayed inside the player, pause, and autosave. This caught and fixed treating normal coroutine cancellation as a gameplay error. The macOS development app image builds and its packaged `--smoke-test` passes without a repository library path. Android arm64 Clippy passes for the platform-specific JNI/AAudio code. No physical Android device is connected (only the dedicated emulator), and Linux remote CI has not run. Physical stereo/latency/gamepad testing and Linux acceptance cannot be completed in the current attached-device environment. Desktop milestone 5 remains in progress, with gamepad integration and the full desktop picture/save-slot UI explicitly unfinished. SNES comparison/selection and Dolphin milestones have not started.


**Priority correction.** The user does not own an Android tablet and explicitly requested PC priority. Continue on **macOS and Linux only** (confirmed in the follow-up answer); Android emulator validation is sufficient for current work. A coworker may test physical Android hardware later, with device-specific changes made then. Do not stop implementation again merely because a physical tablet is unavailable.


**PC-priority continuation — September 12, 2026.** macOS/Linux were explicitly reconfirmed; no Windows target was added. Desktop gamepads now use an optional SDL-backed JNI feature with two stable controller slots, standard button/stick mapping, disconnection cleanup and runtime capability masking. Actual SDL virtual-controller tests cover press/release, stick input and removal. The desktop feature builds into `target/compose-desktop`, separate from Android's host library. Fast-forward preserves held keyboard input; pause/rewind/focus loss clear it.

Desktop now offers ten manual slots with PNG previews plus a separate autosave, 5/15-second rewind jumps, and persistent aspect/edge trimming through shared geometry. Slot zero preserves SDL's `<id>.state`; battery paths remain interoperable. Full filter parity and controller-remapping UI remain future desktop work. Legacy Android migration now preserves the original unversioned index as `index-v0.backup.json` before its first schema-1 write; the migration test checks those exact backup bytes. Rollback is manual and must reconcile any games added after that backup, rather than letting an old build rewrite schema 1.

The isolated SNES comparison is in **`docs/SNES-CORE-COMPARISON.md`**, with a reproducible Emulia-authored harness under `scripts/snes-spike`. The upstream checkout and executable are outside the shipping workspace. Rust 1.98.1 was installed alongside the existing default toolchain solely for that evaluation. Pinned jgenesis passes generated-ROM frame/audio callbacks, state/video/audio replay, history restore and Android arm64 compilation. Following the probe's evidence, runtime history now handles length changes using zero-padded XOR deltas with the predecessor length; full snapshots are no longer retained at each length change. All 120 tested SNES frames fit the 64 MiB accounted budget. These are development-probe results, not real-game, chip, audio-fidelity or sustained 8× qualification.

Current validation: 163 Rust workspace tests reported passing (the external nestest fixture still internally skips), strict Clippy for changed runtime/native crates passes, 4 desktop tests pass, Android assemble/lint/37 JVM tests pass. Packaged macOS JNI/controller smoke and the latest Android emulator/host JNI results are finalized below. Linux CI jobs are configured but no remote results have been obtained for this uncommitted branch.

**Historical decision boundary (resolved by the user below):** choose jgenesis with GPL-compatible distribution or original SNES-core development, using the comparison document and an initial game/chip compatibility set. This is the current decision boundary. Do not introduce a shipping GPL dependency or silently choose an original-core rewrite before the user decides. Physical Android availability is explicitly NOT a blocker.

Finalized checks for the PC-priority continuation: packaged macOS JNI/controller smoke passed, host Kotlin/JNI **17 tests passed**, and the final Android emulator run **16 tests passed**. The dedicated emulator was stopped after validation. All changes remain uncommitted on `multisystem/nes-core-boundary`.


### September 13, 2026 — jgenesis selected and first desktop integration

The user selected **jgenesis with GPL-compatible distribution**, resolving the
historical decision boundary above. The first real-game qualification target is
**Super Mario World**, selected by the user. No SNES game ROM is present in the
workspace; obtaining its local path from the user is the next dependency. Do not
substitute generated-ROM results for this game's compatibility results.

The pinned revision `b1419eface3147568b2247d33b6bdb6695adfe06` is vendored under
`vendor/jgenesis` (nine required crates). The patch ledger contains only the
narrowed workspace membership and host accessors for immediate battery SRAM,
exact-length imports and standard-cartridge detection. Upstream hardware/timing
code remains unchanged. Rust 1.98.1 is now pinned for its required language support.
Native integration and `cores/snes-adapter` declare GPL-3.0-only; desktop resources
include GPL text and third-party notices. CI app-image artifacts include an archive
of the corresponding application checkout. These images remain unpublished.

`cores/snes-adapter` implements video/aspect descriptors, 48 kHz stereo, NTSC/PAL
frame timing, two 12-button gamepads, immediate battery saves and ESNS states.
States include the canonical ROM hash, exact upstream revision and payload digest;
invalid states are rejected before replacing the active machine. Restored states
also restore the current preview and clear transient audio. Shared runtime history
and batch execution supply rewind/fast-forward. Standard LoROM/HiROM imports accept
.sfc/.smc and strip copier headers for identity; enhancement chips remain gated both
by the header and upstream's resulting cartridge type. NES hardware, ANES bytes,
legacy IDs, Android JNI entry points and SDL save paths remain intact.

Compose desktop now imports and plays SNES, preserving separate SNES game/save IDs.
Keyboard Z/X maps A/B, A/S maps X/Y and Q/W maps L/R; two SDL gamepads expose all
buttons. Desktop manual slots, thumbnails and pause/leave autosaves use the existing
generic persistence path. Android shipping builds remain NES-only for this
PC-priority pass, though the SNES-enabled native crate passes an Android arm64 check.

Validation after integration:

| Check | Result |
|---|---|
| Rust workspace | 165 tests passed; external nestest fixture still internally skips |
| Desktop-feature native controller tests | 2 passed |
| SNES tests (included in workspace count) | 4 passed: LoROM replay/video/audio/history, immediate SRAM and failed import isolation, corrupt/wrong-content/version rejection, HiROM/PAL and input validation |
| Strict Clippy, SNES adapter + desktop native integration | Passed; no claim of full-workspace strict Clippy |
| Compose desktop | 6 tests passed, including SNES player/pause/autosave and import/restore/error isolation |
| Packaged macOS app | Built; bundled NES/SNES JNI, restore, rewind, controller and index smoke passed |
| Android | Assemble debug/test APKs, lint and 37 JVM tests passed |
| Dedicated API 35 ARM64 emulator | All 16 tests passed |
| Host Kotlin/diagnostic JNI | 17 tests passed |
| Android arm64 SNES-enabled native compile check | Passed |

Super Mario World qualification should record ROM hash/header and region, then
verify normal level play, both audio channels, pause/focus/resume, battery save and
fresh reopen, manual and auto state restore, deterministic input replay, rewind
followed by resumed play and save, fast-forward followed by normal audio, and
history exhaustion. Measure sustained real-time and fast-forward behavior on the
actual supported host. A generated backdrop program is not performance or audio
fidelity evidence for Super Mario World. No commercial ROMs are bundled.

Milestone 7 remains in progress pending this real-game qualification. Linux CI is
configured but has not run remotely for this uncommitted branch. Desktop filter
parity/remapping and sustained playback qualification remain open. Android hardware
verification stays deferred to the coworker and is NOT a blocker. Dolphin remains
a later external-launcher milestone. All changes remain uncommitted/unpushed on
`multisystem/nes-core-boundary`.


### September 13, 2026 — Super Mario World local-ROM checks

The user supplied Super Mario World (USA). Its canonical SHA256 is
`0838e531fe22c077528febe14cb3ff7c492f1f5fa8de354192bdff7137c27f5b`.
The earlier missing-ROM dependency is resolved. See
[SNES-SMW-QUALIFICATION.md](SNES-SMW-QUALIFICATION.md) for results, reproduction and
remaining acceptance. The local ROM and generated images/states are outside the repo.

The game reaches the first level; captured adapter and actual Compose player images
were inspected. Exact 120-frame video/audio/state replay, fresh-core restore, battery
byte round-trip, rewind exhaustion, fast-forward muting and normal audio recovery
pass. Seven desktop tests pass with the optional supplied-ROM test enabled, including
focus loss/resume and exact autosave persistence. First-level short adapter timing
samples are about 117–120 fps with history, approximately 2× normal rate; sustained
4× is not established. An in-game progress save, long gameplay/audio assessment and
Linux execution remain unverified. Do not report the whole SNES milestone complete.

The reproducible local-ROM tool is `cores/snes-adapter/examples/qualify.rs`, with
input-only `scripts/snes-smw-sequence.txt`. No game data is bundled. Normal CI skips
the optional local-ROM test explicitly when `EMULIA_SNES_ROM` is absent. Existing
Android results remain those of the integration pass; no Android production code
changed during this qualification work.


### Desktop packaging sidequest

The user requested proper macOS application and Linux desktop build paths before
continuing emulation milestones. `scripts/build-desktop.py macos` now builds the
branded `.app`, DMG and app ZIP; `deb`, `rpm` and `linux` build Linux installers with
freedesktop launchers, PNG/SVG theme icons and AppStream metadata for GNOME/KDE.
All include their Java runtime, native core, notices and corresponding source.
The existing brand mark is regenerated for desktop by `scripts/build-desktop-icons.py`.
See [DESKTOP-PACKAGING.md](DESKTOP-PACKAGING.md) for commands and acceptance limits.

macOS arm64 bundle/icon/JNI checks and DMG verification passed. Linux arm64 DEB/RPM
builds completed in a disposable Debian 12 container; the DEB installed and passed
packaged JNI and real window class/icon checks under Xvfb/Openbox. Six regular
Compose tests passed on each OS; the optional local-ROM test skipped in these runs.
An RPM installation on an RPM distribution and full GNOME/KDE session testing
remain unverified. Public macOS signing/notarization requires a Developer ID;
no signing credentials, release upload or publication was attempted. Installer
artifacts are under ignored `desktop-ui/build/packages`. All source changes remain
uncommitted on the existing work branch. The emulator milestone work can resume
after this sidequest.
