# Super Mario World qualification

Tested September 13, 2026 using the user's local ROM. ROM data, rendered frames,
states and battery files remain outside the repository and distributable packages.

- Canonical SHA256: `0838e531fe22c077528febe14cb3ff7c492f1f5fa8de354192bdff7137c27f5b`
- Header: `SUPER MARIOWORLD`; 524,288 bytes; LoROM; 2 KiB battery SRAM.
- Adapter: jgenesis SNES at `b1419eface3147568b2247d33b6bdb6695adfe06`.
- Timing: 60.098800 Hz; 48 kHz stereo; ESNS state version 1.

## Current PR #6 desktop-shell integration

The authoritative frontend is the existing Rust/SDL/egui shell, on the
`multisystem/desktop-shell` branch targeting `feat/desktop-shell` through PR #7.
Earlier Compose screenshots and tests describe a superseded frontend.

| Check | Evidence |
|---|---|
| Scripted first-level route | Local-ROM desktop session test reaches Yoshi's Island 1; saved thumbnail inspected |
| State replay | Save slot load followed by 120 identical inputs reproduces the exact final serialized state |
| Restored preview | Slot load immediately restores the saved RGBA frame |
| Rewind | Reverses all 120 frames exactly to the saved checkpoint; exhaustion holds |
| Autosave | Closing/reopening through the desktop library restores the same frame |
| Actual Linux window | Installed Debian arm64 app renders the game, original pause panel and live settings preview under Xvfb/Mesa |
| Stereo and persistence | Packaged macOS and Linux executable smoke tests use a generated SNES ROM with the real SDL audio queue; SRAM survives restart and copier-header import deduplicates |
| Session parity | Generated SNES session test covers slots/thumbnails, reset retaining SRAM, rewind exhaustion, muted fast-forward and normal audio recovery |
| Input | Keyboard mapping test covers all 12 controls and separate NES/SNES profiles; SDL virtual-controller test covers shoulders versus time triggers |

The original PR #6 NES engine is byte-for-byte unchanged in
`desktop/src/engine/nes.rs`. Existing desktop tests and the original NES executable
smoke checks continue to pass. Linux desktop integration here is exercised in a
Debian container, not a physical GNOME/KDE session or a Wayland compositor.

## Reproduce

Use the pinned Rust toolchain; no Java runtime is required for the desktop shell.

```sh
EMULIA_SMW_ROM='/path/to/Super Mario World (USA).sfc' \
  cargo test --release --locked -p nes-desktop \
  super_mario_world_desktop_session -- --ignored --nocapture

cargo run --release --locked -p nes-desktop -- '/path/to/Super Mario World (USA).sfc'
```

The optional test uses a separate temporary library and prints its artifact path.
It is explicitly ignored in ordinary CI because the ROM is not distributed.

The independent adapter probe remains available:

```sh
cargo run --locked --release -p snes-adapter --example qualify -- \
  '/path/to/Super Mario World (USA).sfc' /tmp/emulia-smw-check scripts/snes-smw-sequence.txt
```

Earlier adapter measurements on macOS arm64 captured 600 first-level frames at
about 117 fps with history and 120 fps in four-frame batches. These are short
adapter samples, about 2× normal speed, not a sustained 4× desktop claim. The
adapter probe also checks per-frame video/audio hashes, finite distinct stereo
channels, fresh-core restore and battery byte round-tripping.

## Remaining acceptance

Complete an in-game save and verify progress after a battery-only restart;
exercise later levels, longer sessions and manual gameplay; listen for glitches
and assess input latency. An SRAM byte round-trip is not proof of an in-game
save checkpoint. Enhancement chips remain gated. Physical controllers, actual
GNOME/KDE/Wayland sessions and macOS Intel are additional qualification targets.
Android recovery from the previous checkpoint remains separate follow-up work;
physical tablet checks stay deferred to the coworker.
