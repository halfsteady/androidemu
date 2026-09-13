# Super Mario World: initial macOS qualification

Tested September 13, 2026 on macOS arm64, using the user's local ROM. ROM data,
rendered frames, states and battery data are kept outside the repository.

- Canonical SHA256: `0838e531fe22c077528febe14cb3ff7c492f1f5fa8de354192bdff7137c27f5b`
- Header title: `SUPER MARIOWORLD`; 524,288 bytes; LoROM; 2 KiB battery SRAM.
- Adapter: `jgenesis-snes`, upstream `b1419eface3147568b2247d33b6bdb6695adfe06`.
- Native frame rate: 60.098800 Hz; stereo output at 48 kHz; ESNS state version 1.

## Observed results

| Check | Evidence |
|---|---|
| Boot, menus, overworld, first level | Rendered checkpoints inspected; first-level image also captured from the Compose player |
| Input | Scripted menu selection, overworld movement, level entry and right/jump change gameplay |
| Restore/replay | 120 identical inputs reproduce the exact final state and every frame's video/audio hash |
| Restored preview | Matches the saved frame immediately; transient audio clears |
| Fresh-core restore | A newly constructed adapter accepts the disk checkpoint and reproduces its state |
| Battery import/export | 2,048 bytes round-trip into a fresh adapter; this does **not** verify game progress saved at an in-game save point |
| Rewind | 600 captured frames retained in the 64 MiB history budget in the sampled scene; exhaustion holds without advancing |
| Fast-forward and recovery | Multi-frame batches deliver no audio to the host; normal single-frame playback produces audio after restore |
| Stereo | Both channels contain finite, nonzero samples and differ during gameplay; this is not an audio-fidelity comparison |
| Desktop lifecycle | Local-ROM Compose test loads a level checkpoint, renders, pauses on focus loss, resumes, writes an exact autosave and exits without audio initialization errors |
| Regression | Seven desktop tests pass with the local-ROM test enabled; strict adapter/example Clippy passes |

The first-level adapter-only measurements captured 600 frames at approximately
117 fps with history and approximately 120 fps in batches of four. These short
samples are about 2× normal speed, not sustained 4× qualification or a Compose
render/audio benchmark. Generated-ROM performance is not substituted for game data.

## Reproduce locally

Use the rustup Cargo proxy and Java 17. Provide your own ROM; the script contains
only frame counts and input masks. It assumes a fresh power-on without a battery
save. Checkpoints should be inspected when changing the ROM revision or core.

```sh
SMW_ROM="$HOME/Downloads/Super Mario World (USA).sfc"
cargo run --locked --release -p snes-adapter --example qualify -- \
  "$SMW_ROM" /tmp/emulia-smw-check scripts/snes-smw-sequence.txt

EMULIA_SNES_ROM="$SMW_ROM" \
EMULIA_SNES_STATE=/tmp/emulia-smw-check/level-start.state \
EMULIA_SNES_SCREENSHOT=/tmp/emulia-smw-desktop.png \
  ./android/gradlew -p android -PdesktopOnly :desktop-ui:test
```

The example also accepts an initial state after the script argument and
`--capture-only` to inspect a shorter input sequence without running the assertions
and timing samples. Outputs include PPM frames and ESNS states. The optional desktop
test uses a temporary library and leaves the user's normal library untouched. Without
`EMULIA_SNES_ROM`, that one test explicitly skips; generated-ROM tests still run.

## Remaining acceptance

Complete an in-game save and verify progress after a battery-only restart; exercise
later levels, longer sessions and manual gameplay; listen for glitches and assess
input latency. Linux runtime/package execution and sustained playback/performance
qualification remain open. Enhancement chips remain gated. These results qualify
an initial integration smoke test, not the entire game or the completed SNES milestone.
