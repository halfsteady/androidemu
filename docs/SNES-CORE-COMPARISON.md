# SNES core comparison — jgenesis selected

Desktop work prioritizes **macOS and Linux**. Android emulator coverage remains in
scope; physical Android verification is deferred to the user's coworker. Windows
is excluded from the current targets.

The user selected **jgenesis with GPL-compatible distribution** on September 13,
2026. The pinned backend is now vendored and included in Compose desktop builds.
Android remains NES-only for this desktop-first integration. See
[THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md) and the latest handoff record.
The options and isolated-probe results below record the evidence for that decision.

## Options

| Path | Evidence and fit | Remaining cost / decision |
|---|---|---|
| Integrate jgenesis | Rust backend compiles separately from its UI; generated-ROM frame, audio, serialization and restore/replay probe passes on macOS arm64; Android arm64 compile check passes | GPLv3 distribution decision; adapter/state-envelope work; upstream toolchain and API maintenance; real-game and enhancement-chip qualification; rewind capture performance |
| Build an original SNES core | The new Emulia core/runtime boundary can host it without another frontend rewrite | Implement and validate CPU, bus, DMA, PPU, SPC700/DSP, cartridge maps, timing and supported chips; no original SNES hardware implementation or compatibility evidence exists yet |
| Snes9x/libretro | Established C/C++ backend and existing callback interface | Custom noncommercial terms; C callback/ownership integration; no Emulia build/performance spike performed here |
| bsnes | Existing C++ SNES implementation with broad hardware scope | GPLv3-or-later distribution decision; C++ integration and host performance work; not built or benchmarked here |

The license descriptions come from the projects' actual notices:
[jgenesis GPLv3](https://github.com/jsgroth/jgenesis/blob/b1419eface3147568b2247d33b6bdb6695adfe06/LICENSE),
[Snes9x license](https://github.com/snes9xgit/snes9x/blob/7a8878f1306f65594c30b7d86dee41d972c2e495/LICENSE),
and [bsnes license](https://github.com/bsnes-emu/bsnes/blob/master/LICENSE.txt).
Choosing an upstream backend does not mean its combined executable can be shipped
under Emulia's current MIT declaration alone. The user accepted the
jgenesis path; the native integration and SNES adapter declare GPL-3.0-only.

**Recommendation:** pursue the jgenesis adapter if GPL-compatible distribution is
acceptable and delivering SNES compatibility is the priority. Choose original-core
development if retaining an independently implemented core is more important than
time to compatibility. Neither path is qualified for release by this document.

## What the isolated probe actually established

Pinned jgenesis revision: `b1419eface3147568b2247d33b6bdb6695adfe06`.
No upstream source patches were made. Emulia-authored probe source and its resolved
Cargo lockfile live under `scripts/snes-spike`; the generated Cargo project and
upstream checkout are outside the application workspace.

- Rust 1.95 fails on upstream's `float_algebraic` methods. Rust 1.98.1 builds the
  backend successfully; it was installed side by side without changing the
  repository's default toolchain. These methods were stabilized in Rust 1.98.
  See the [Rust API documentation](https://doc.rust-lang.org/std/primitive.f64.html#method.algebraic_add).
- The original generated 32 KiB LoROM program changes the backdrop. The probe
  drives upstream's renderer, stereo-audio and in-memory save callbacks without
  its desktop frontend, and observes a 256×224 frame and 798 stereo sample frames
  in the last measured frame at a requested 48 kHz output rate.
- After warm-up, serialize, execute ten frames, restore and execute those same
  ten frames again: serialized state bytes, video pixels and audio samples match.
  This is replay within one initialized session, not a cross-machine or
  cross-version determinism claim. The fixture does not program the audio DSP,
  exercise battery writes, or qualify audible channel correctness.
- Upstream `load_state` reattaches ROM/coprocessor resources from the running core.
  A production adapter must decode and validate the complete candidate before
  calling it, including Emulia's core/version/content identity envelope. It must
  not expose arbitrary decoded state as trusted input. See the pinned
  [SNES API](https://github.com/jsgroth/jgenesis/blob/b1419eface3147568b2247d33b6bdb6695adfe06/backend/snes-core/src/api.rs).
- Upstream's native driver uses little-endian fixed-integer bincode encoding with
  a 100 MiB decode cap. Matching those settings gives a constant **1,294,569-byte**
  snapshot in this fixture. Bincode's default variable-integer encoding instead
  produced sizes from 1,255,883 to 1,256,399 bytes. State encoding is therefore a
  material integration choice; no promise of compatibility with upstream save
  files is made. See the pinned
  [driver configuration](https://github.com/jsgroth/jgenesis/blob/b1419eface3147568b2247d33b6bdb6695adfe06/frontend/jgenesis-native-driver/src/mainloop.rs).
- The initial full-predecessor fallback retained only 71 frames in the 64 MiB
  runtime budget under variable-integer encoding. The runtime now zero-pads
  differing lengths for XOR deltas and records the predecessor length. A focused
  regression test covers sparse changing-size states. The final fixed-integer
  probe retained and successfully restored all **120 frames**, accounting for
  **49,503,842 bytes**, including conservative scratch reservations.
- One local fixed-integer run measured **300.53 fps** without snapshots/history
  and **167.53 fps** with per-frame serialization and history capture. Other
  exploratory runs differed while builds were active. These are short generated-
  ROM observations on the development Mac, not representative-game benchmarks,
  controlled speedup comparisons, or proof that an eight-frame fast-forward
  request can sustain 8× speed. Capture performance needs further work.
- `cargo +1.98.1 check --locked -p snes-core --target aarch64-linux-android`
  passes. This is a compilation check, not an Android SNES runtime test.

The adapter still needs gamepad mapping, battery/auxiliary save ownership,
descriptor conversion, bounds/error handling, versioned states and recovery tests.
Coprocessor ROM resources must be supplied by the user where required, not bundled
by assumption. The fixture establishes none of the ordinary-game or enhancement-
chip compatibility matrix.

## Reproduce

Use an external checkout at the pinned revision, with Rust 1.98.1 installed:

```sh
git clone https://github.com/jsgroth/jgenesis.git /tmp/emulia-jgenesis-spike
git -C /tmp/emulia-jgenesis-spike checkout b1419eface3147568b2247d33b6bdb6695adfe06
rustup toolchain install 1.98.1 --profile minimal
python3 scripts/snes-spike/run.py --upstream /tmp/emulia-jgenesis-spike --target-dir /tmp/emulia-snes-probe/target
```

The script verifies the revision and rejects an upstream checkout with source
modifications. It uses its own lockfile and temporary manifest. It does not edit
Emulia's root Cargo manifest or switch the default Rust toolchain.

## Original-core development plan

An original implementation should proceed as a separate hardware project:

1. Deterministic power-on state, cartridge maps, bus and 65C816 execution, with
   instruction/bus trace tests and complete state encoding from the start.
2. Master-clock scheduling, interrupts, DMA/HDMA and CPU/PPU bus interactions.
3. PPU registers, background/sprite modes, palettes, windows, color math and
   interlace/hires behavior, validated with generated tests and redistributable
   hardware test programs.
4. SPC700, DSP voices, envelopes, echo and stereo timing, with audio-state replay
   tests and resampling owned by the host runtime.
5. An explicit ordinary-cartridge compatibility set, then each selected
   enhancement-chip class as its own implementation and validation milestone.
6. Save/restore completeness, failed-restore isolation, rewind branching and
   exhaustion, 5/15-second jumps when history permits, and measured fast-forward
   with history enabled on both desktop targets and Android emulator.

This is many subsystems of new emulator work, not another adapter milestone. The
existing NES hardware is not a shortcut to a complete SNES implementation. A
reliable delivery estimate requires choosing the initial games/chips and deciding
how much original hardware work the project intends to own.

## Selected implementation and remaining qualification

Use jgenesis at the pinned revision, behind `cores/snes-adapter`. Initial imports
are standard LoROM/HiROM, including copier-header variants, with two ordinary
12-button gamepads. Enhancement chips and other peripherals remain gated.
Save states use an ESNS envelope tied to the ROM hash and upstream revision;
NES identities and ANES bytes remain unchanged. GPL notices accompany desktop
resources, and the vendor patch ledger records the narrow host API extensions.

Generated programs establish host integration and deterministic replay. A real-game
compatibility matrix, sustained audio/performance measurements, and Linux execution
remain necessary before release. Dolphin remains a later external-launcher milestone.

The user selected **Super Mario World** as the first real-game qualification target.
The user subsequently supplied a local ROM. Initial first-level qualification results
and remaining acceptance are recorded in [SNES-SMW-QUALIFICATION.md](SNES-SMW-QUALIFICATION.md).
