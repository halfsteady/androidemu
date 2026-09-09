# Review of PRs #3 and #4 — 2026-09-09

Reviewed PR #3's desktop-player commit `98f040d` and PR #4's accuracy commit
`43a8989`, including their integration at `00e8176`. The local branch was first
fast-forwarded to that PR head; the previous checkout had the branch name but
predated both PRs.

The fixes retain **103/144 AccuracyCoin passes** and **39/39 external regression
ROM passes**. This review does not increase the AccuracyCoin score or establish
complete NES accuracy.

## Findings and fixes

| Priority | Area | Finding | Resolution and evidence |
|---|---|---|---|
| P2 | PR #4, PPU OAM writes | Rendering-time `$2004` writes changed address `$01` to `$04`, losing the byte offset within a sprite. Later writes after rendering was disabled could consequently target the wrong OAM byte. | Increment by four without clearing the low two bits. The corrected original test and a new test covering all 256 addresses failed before the fix and pass afterward. Visible lines, pre-render, blanking and wraparound are covered. |
| P2 | PR #3, desktop autosave | A failed periodic save propagated out of the play loop. A transient file conflict or unavailable save directory could close the game, discarding the live machine when the final save failed too. | Report the error, preserve the running machine and retry on the next interval. The executable smoke test holds a temporary-file conflict across an autosave, confirms the game stays running, removes it, and verifies the clean-exit save. |
| P2 | PR #4, regression executable selection | The checker always executed `target/release/nes-runner`, regardless of where Cargo actually built it. With `CARGO_TARGET_DIR` or a configured target, this could fail or silently test an older binary. | Use Cargo's executable artifact path. The full pinned-ROM regression passed with a separate target directory containing spaces. |
| P3 | PR #4, benchmark protocol | The standalone runner accepted duplicate test result addresses and malformed display names, and could report completion even with unfinished/skipped results. Its status decoder also treated invalid odd values as passes. | Validate unique page/result pointers, bounded printable names, page terminators, test/drawing counts and result tags. Require every test to finish before reporting completion. Synthetic tests cover malformed layouts and completion counters with unfinished results; the actual upstream ROM still runs successfully. |
| P3 | PR #3, pause persistence | The pause/focus-loss path bypassed periodic saving entirely. Recent battery changes could remain unsaved indefinitely while the window was inactive. | Flush when entering pause and continue the retry timer while paused. A test delivers a real SDL focus-loss event and verifies the battery file before resume or final-exit saving. |
| P3 | PR #3, desktop CI coverage | A successful bundled-SDL build and dummy-driver smoke test did not establish that the executable had real desktop video/audio backends. This was reproduced locally with only offscreen/dummy video and no ALSA/PulseAudio. | Add `--list-drivers`, require native backends in CI, and list/install `pkg-config` explicitly. The gate rejects the deficient build and passes with X11 and ALSA compiled in. |

The OAM fix follows the [NESdev OAMDATA documentation](https://www.nesdev.org/wiki/PPU_registers#OAMDATA_-_Sprite_RAM_data_($2004_read/write)):
rendering-time writes increment the sprite index in the upper six bits. The
benchmark parser was checked against the [pinned AccuracyCoin assembly](https://github.com/100thCoin/AccuracyCoin/blob/affc643aa771028510c4427451ecf5bba5e54592/AccuracyCoin.asm).
Its five drawing entries intentionally share scratch RAM at `$03FF`; uniqueness
is required for actual test results, not those drawing entries.

Additional improvements: validate AccuracyCoin's frame-limit argument instead
of silently accepting typos; test the Python regression gate's rejection of
lost passes even when an unrelated new pass keeps the total unchanged; verify
summary/exit-code consistency; retain accuracy reports as CI artifacts; lint
the core and runner alongside desktop. Two existing header-repair expressions
were updated to the equivalent `is_multiple_of` form so workspace Clippy passes
with warnings denied.

## Review coverage

PR #4: CPU bus latch behavior around `$4015`, controller strobe phase, IRQ/BRK
hijacking and late NMIs, OAM register access and sprite-fetch boundaries,
background shifter changes, interaction with DMA and serialization, runner
metadata/result parsing, the pinned baseline, and the regression workflow.
Interrupt tests now sweep all seven arrival cycles for both BRK and hardware
IRQ, checking vectors, return addresses, pushed flags and handler execution.

PR #3: workspace/dependency configuration, the macOS Clang-runtime build script,
SDL lifecycle, rendering/palette conversion, audio queue and muted pacing,
keyboard/focus/pause handling, ROM identity and save locations, SRAM and
save-state loading, atomic replacement and error paths, CLI handling, packaging,
and CI coverage. The platform build files were inspected; actual execution in
this review was on Linux x86-64.

## Validation

| Check | Result |
|---|---|
| `cargo test --workspace --locked --offline` | 105 passed, including the real nestest reference trace and legacy/current state compatibility tests |
| `cargo clippy --workspace --all-targets --locked --offline --no-deps -- -D warnings` | Passed |
| Pinned AccuracyCoin ROM, SHA-256 verified | 103/144; no changed result/status codes compared with the PR head |
| Same AccuracyCoin run using a custom Cargo output directory containing spaces | Passed; 103/144, zero regressions |
| `scripts/check-roms.py` CPU/APU/PPU/MMC3 suite | 39/39 before and after the fixes |
| `python3 scripts/test-accuracycoin.py` | 5 regression-gate tests passed |
| Release builds of runner and desktop | Passed |
| `scripts/check-desktop.py ... --require-native-drivers` | Passed with compiled X11 and ALSA; exercises SDL dummy video/audio, muted pacing, SRAM persistence, malformed input handling and transient autosave recovery |
| Native-backend negative check | Correctly rejected the build lacking desktop video/audio support |
| `git diff --check` | Passed |

External ROMs were kept outside tracked source. The test-ROM collection was
checked out under `/tmp`; nestest's ROM/log were copied into the existing ignored
test-fixture directory. Linux development packages were downloaded and extracted
under `/tmp` for the native-backend build, without changing system packages.

## Remaining limits

The [41 recorded AccuracyCoin failures](accuracycoin-results.json) remain. DMC
DMA arbitration, APU timing and cycle-by-cycle PPU sprite evaluation/fetch
behavior require more work than these corrections. No ROM-specific behavior
was added to the core, and save-state fields/formats did not change.

Compiled backend detection and SDL dummy-driver tests do not validate a real
window manager, GPU, keyboard or speaker. macOS execution, ARM64 builds, physical
audio/input, Android device behavior and PAL/Dendy hardware accuracy were not
validated here. The existing region unit tests pass; they are not substitutes
for hardware conformance suites. The modified GitHub Actions matrix still needs
to run on its hosted platforms.
