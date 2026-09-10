# Phase 1 — implementation and acceptance

The first installable preview is implemented and confirmed playing on the tablet
by hand on 2026-09-07. Phase 1 is **not yet signed off**: the gates below that ask
for a number are still unmeasured, and controllers and the PAL/Dendy paths have not
been walked through deliberately.

## Implemented

- CPU instructions, cycle-based bus, interleaved OAM DMA, DMC sample fetch/stalls,
  interrupt polling, PPU background/sprite rendering, and scrolling and frame
  timing for the machine's region.
- Mappers 0, 1, 2, 3, 4, 7 and 66, bank switching, mirroring, battery RAM,
  MMC3B/C IRQs, and GxROM bus conflicts.
- NTSC, PAL and Dendy timing: per-region CPU clock, PPU-to-CPU ratio, frame
  height, vblank scanline, APU frame-counter steps and noise/DMC period tables.
  The shell requests the region's refresh rate instead of assuming 60.0988 Hz.
- All five APU channels, nonlinear mixing, 1024-tap FIR anti-aliasing before
  48 kHz output, DC removal and analog low-pass filtering. No per-frame allocation
  in the core; snapshot allocation is outside frame execution.
- Version-3 deterministic snapshots with v1/v2 migration, checksum and ROM/board
  identity validation, transactional loading, and exact-size battery-save restoration.
- Compose game shelf, single-file SAF import into private storage, GLES 3 rendering,
  Choreographer scheduling, a region-derived surface frame rate, touch controls and
  standard Android USB/Bluetooth/keyboard input for two players. Portrait controls
  reflow. A guided four-step wizard maps A/B/Select/Start per controller, saved by
  vendor/product/descriptor and applied by scan code, which is what makes cheap USB
  adapters usable; Start also resumes from the pause panel through that mapping.
  Full screen hides the system bars, with touch controls optional while in it.
- Direct AAudio low-latency callback, exclusive/shared fallback, bounded lock-free
  sample queue, device-rate conversion and limited buffer-fill rate correction.
  AAudio is used directly because API 29+ is the floor; Oboe's older-device backend
  is not needed. This is a deliberate change from the original Oboe wrapper plan.
- Rewind: a hold-to-undo control that runs the game backwards a frame at a time.
  Each frame is stored as a run-length-encoded XOR against the one before it and
  the window is bounded in bytes, not frames, so a still screen buys about a
  minute and a full-screen scroll rather less. Loading a savestate abandons the
  chain rather than rewinding into a timeline that was never played.
- Reset game in the Android pause menu starts a fresh power-on, keeps battery
  saves and manual slots, clears rewind and replaces the autosave. A failed ROM
  read or cartridge mismatch leaves the running session intact.
- Ten manual save slots with screenshots and timestamps, overwrite confirmation,
  a separate autosave on pause/background, resume from the shelf, atomic save and
  SRAM file replacement, and clear error feedback. No network permission or ROMs
  in the app. Missing artwork uses a designed placeholder and then a saved frame.

## Verification

- Rust workspace tests: 146 passing, including the real nestest reference trace,
  GxROM banking under bus conflicts, and per-region frame length, frame height,
  vblank scanline, savestate timing restoration and the rewind chain. Without the optional nestest
  files that harness reports that it skipped. A golden-hash test pins the layout
  of version-1 snapshots, so the saves made with the first preview keep loading.
- Public ROMs: all 39 selected ROMs pass (16 CPU instruction, 8 APU, 10 PPU
  vblank/NMI and 5 MMC3 tests). `scripts/check-roms.py` runs these externally.
  Tested collection revision: `95d8f621ae55cee0d09b91519a8989ae0e64753b`.
- Kotlin/JNI host-JVM tests exercise actual native rendering, deterministic
  save/restore, battery data, exception translation, invalid buffers, rejected
  corrupt states, and rewinding back through the exact frames just played. Android audio is excluded from the host library.
- arm64 and x86_64 native builds, Android app/test APK builds, Android lint.
- Device instrumentation is written but **not executed successfully** here. The
  available emulator lacks KVM and repeatedly loses Android system services before
  installation. It was stopped; no existing virtual-device data was wiped.

## Remaining acceptance gates

Play → pause → save → load → resume, and touch input, are confirmed by hand on the
OnePlus Pad 3 (owner report, 2026-09-07). What is left is everything that produces
a number, plus the paths that need particular hardware or particular ROMs.

- [ ] Run the instrumentation play → pause → save → load → resume test on a stable
  emulator or tablet; review portrait and landscape screenshots. It has not been
  executed anywhere yet — the hand check above is not the same evidence.
- [ ] Play representative, legally supplied games on each tier-1 mapper. Confirm
  sprite rendering against captures; sprite evaluation and fetch now run per dot.
- [ ] Confirm real USB adapters, Bluetooth reconnects, two-player assignments and
  absence of stuck buttons after unplug, backgrounding and touch cancellation.
  Run the mapping wizard on each adapter and confirm the profile survives a
  replug and a restart, and that Start resumes from the pause panel.
- [ ] Play a PAL and a Dendy game end to end. Region timing passes in the core,
  but the 50 Hz surface request and audio rate correction are unmeasured on the
  panel, and no region-specific ROM suite is run.
- [ ] Measure sustained frame pacing and input-to-photon latency on the OnePlus Pad 3.
- [ ] Measure what rewind costs on the tablet. Every frame is serialised and
  XOR-encoded, roughly 95 KB a frame before encoding, and neither the CPU cost
  nor the real depth of the 64 MB window has been measured on device.
- [ ] Measure underruns/output latency, test output-route changes and long sessions.
  The figure is now on screen in Settings and comes out of `Native.audioStats()`,
  so this gate is down to reading it off the tablet. The queue target adapts and
  is measured in frames of audio: about 12-14 ms end to end, down from 20.8 ms.
  The sub-10 ms target is **not achieved**, and [the audio note](AUDIO.md) records
  why it needs emulation paced against the audio clock rather than a smaller
  constant.
- [ ] Broaden accuracy checks beyond the 144/144 pinned AccuracyCoin tests,
  particularly OAM decay, PAL sprite evaluation and rendering-time bus conflicts.
  Secondary OAM/overflow, DMC/OAM arbitration and repeated controller reads now
  have passing regression coverage. MMC6 and other board revisions remain outside
  the common MMC3 implementation.

Color emphasis, raw USB HID fallback, D-pad/axis remapping, folder/ZIP import and
box-art packs remain planned follow-up work. Rewind, fast-forward, box art and
picture settings landed in [Phase 2](PHASE-2.md). The mapping wizard covers the
four face/menu buttons; directions still come from the standard D-pad and stick
handling, and the shoulder buttons are reserved for the time controls.

## Save format and recovery

Snapshots start with `ANES`, a little-endian version (currently 3), ROM hash,
mapper and board fingerprint, followed by the CPU, bus/devices and mutable mapper
state. A trailing FNV-1a checksum detects accidental corruption; it is not an
authentication mechanism. ROM bytes are not embedded. All decoding is bounded,
loads validate device invariants, and a candidate machine replaces the current
one only after validation. Future versions must add an explicit decoder/migration
branch; unknown versions fail without changing the running machine.

Version 2 appends the CPU internal data latch, per-bit PPU open-bus retention
timestamps and pending APU IRQ acknowledgement after the original mapper data.
Version 3 appends DMA timing, controller output-enable and PPU pipeline state,
including pending register effects and sprite flags. Versions 1 and 2 still load,
converting static sprite records to live counters/shifters and initializing absent
fields with compatible defaults. Regression tests load fixtures from both released writers and replay their migrated states.
Older app versions cannot read newly written v3 snapshots; battery saves keep
their existing format.

On Android each ROM hash has a private directory under `files/library/` containing
`game.nes`, `battery.sav`, `auto.state`/`auto.png`, and `slot-0` through `slot-9`
state/PNG pairs. Android `AtomicFile` protects each file replacement. Screenshot
and state are separate files, so a crash between them can leave an older thumbnail;
it does not replace or corrupt the saved machine state. Backup export UI is planned.
