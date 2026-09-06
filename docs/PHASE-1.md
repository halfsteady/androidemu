# Phase 1 — implementation and acceptance

The first installable preview is implemented. Phase 1 is **not yet signed off**:
physical-device play, controllers, pacing and audio latency still need validation.

## Implemented

- CPU instructions, cycle-based bus, interleaved OAM DMA, DMC sample fetch/stalls,
  interrupt polling, PPU background/sprite rendering and NTSC scrolling/timing.
- Mappers 0, 1, 2, 3, 4 and 7, bank switching, mirroring, battery RAM, MMC3B/C IRQs.
- All five APU channels, nonlinear mixing, 1024-tap FIR anti-aliasing before
  48 kHz output, DC removal and analog low-pass filtering. No per-frame allocation
  in the core; snapshot allocation is outside frame execution.
- Version-1 deterministic snapshots, checksum and ROM/board identity validation,
  transactional loading, and exact-size battery-save restoration.
- Compose game shelf, single-file SAF import into private storage, GLES 3 rendering,
  Choreographer scheduling, 60.0988 Hz surface request, touch controls and standard
  Android USB/Bluetooth/keyboard input for two players. Portrait controls reflow.
- Direct AAudio low-latency callback, exclusive/shared fallback, bounded lock-free
  sample queue, device-rate conversion and limited buffer-fill rate correction.
  AAudio is used directly because API 29+ is the floor; Oboe's older-device backend
  is not needed. This is a deliberate change from the original Oboe wrapper plan.
- Ten manual save slots with screenshots and timestamps, overwrite confirmation,
  a separate autosave on pause/background, resume from the shelf, atomic save and
  SRAM file replacement, and clear error feedback. No network permission or ROMs
  in the app. Missing artwork uses a designed placeholder and then a saved frame.

## Verification

- Rust workspace tests: 54 passing, including the real nestest reference trace.
  Without the optional nestest files its harness reports that it skipped.
- Public ROMs: all 39 selected ROMs pass (16 CPU instruction, 8 APU, 10 PPU
  vblank/NMI and 5 MMC3 tests). `scripts/check-roms.py` runs these externally.
  Tested collection revision: `95d8f621ae55cee0d09b91519a8989ae0e64753b`.
- Kotlin/JNI host-JVM tests exercise actual native rendering, deterministic
  save/restore, battery data, exception translation, invalid buffers and rejected
  corrupt states. Android audio is excluded from the host library.
- arm64 and x86_64 native builds, Android app/test APK builds, Android lint.
- Device instrumentation is written but **not executed successfully** here. The
  available emulator lacks KVM and repeatedly loses Android system services before
  installation. It was stopped; no existing virtual-device data was wiped.

## Remaining acceptance gates

- [ ] Run the instrumentation play → pause → save → load → resume test on a stable
  emulator or tablet; review portrait and landscape screenshots and touch input.
- [ ] Play representative, legally supplied games on each tier-1 mapper. Confirm
  sprite rendering against captures; the current sprite selection is scanline-based.
- [ ] Confirm real USB adapters, Bluetooth reconnects, two-player assignments and
  absence of stuck buttons after unplug, backgrounding and touch cancellation.
- [ ] Measure sustained frame pacing and input-to-photon latency on the OnePlus Pad 3.
- [ ] Measure underruns/output latency, test output-route changes and long sessions.
  The frame-fed audio queue currently targets roughly 20 ms, so the original
  sub-10 ms end-to-end audio target is **not achieved or claimed**.
- [ ] Broaden accuracy checks for secondary OAM/overflow quirks, OAM decay,
  DMC/OAM collision arbitration and repeated controller reads. MMC6 and other
  board revisions are not covered by the common MMC3 implementation.

PAL/Dendy playback is rejected by the Android shell until region-specific timing
is implemented. Color emphasis, raw USB HID fallback/remapping, folder/ZIP import,
box-art packs, rewind and kid-mode locking remain planned follow-up work.

## Save format and recovery

Snapshots start with `ANES`, a little-endian version (currently 1), ROM hash,
mapper and board fingerprint, followed by the CPU, bus/devices and mutable mapper
state. A trailing FNV-1a checksum detects accidental corruption; it is not an
authentication mechanism. ROM bytes are not embedded. All decoding is bounded,
loads validate device invariants, and a candidate machine replaces the current
one only after validation. Future versions must add an explicit decoder/migration
branch; unknown versions fail without changing the running machine.

On Android each ROM hash has a private directory under `files/library/` containing
`game.nes`, `battery.sav`, `auto.state`/`auto.png`, and `slot-0` through `slot-9`
state/PNG pairs. Android `AtomicFile` protects each file replacement. Screenshot
and state are separate files, so a crash between them can leave an older thumbnail;
it does not replace or corrupt the saved machine state. Backup export UI is planned.
