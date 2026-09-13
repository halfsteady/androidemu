# Dolphin rendering experiment

This macOS-only, opt-in build hosts Dolphin 2606a inside the existing Emulia
Rust/SDL/egui window. It is a feasibility probe, not a replacement for the external
launcher. Default builds and the normal app/DMG/deb/RPM paths do not enable it.

Requirements: the repository's pinned Rust toolchain, Python 3.11+, Git, CMake,
Xcode command-line tools, and network access for pinned upstream sources.

```sh
python3 scripts/dolphin-spike/build.py
python3 scripts/dolphin-spike/run.py \
  --app target/dolphin-spike/EmuliaDolphinProbe.app \
  --data-dir /tmp/emulia-embedded-library \
  '/path/to/Animal Crossing (USA).iso'
```

Use `--work-dir DIRECTORY` to relocate the checkout, CMake build, isolated Cargo
output and probe app. The builder verifies the Dolphin commit and refuses to
replace unrelated edits in its CMake entry file. It initializes pinned
submodules and adds only this host bridge to the Dolphin build. Repeated builds
reuse their caches. Resources and notices are copied into the local app, whose
ad-hoc signature is verified. System/Homebrew dynamic dependencies are not
bundled, so this is not a portable app distribution.

The experiment requires both Cargo feature `dolphin-embed-probe` and
`EMULIA_DOLPHIN_PROBE_LIB` pointing at its native library. `run.py` supplies the
latter. Ordinary shelf launches still use the external launcher. The embedded
game is chosen on the command line, once per process. Return/Enter is Dolphin's
default GameCube Start mapping; X is A. The prototype provides Escape and a
Back to shelf button. No rewind, fast-forward, state-slot UI or controller wizard
is supplied for this embedded game.

Each launch creates a fresh temporary Dolphin profile and prints its path. It
selects Metal, enables performance counters, disables frame dumping/analytics,
and acknowledges the supplied NKit fixtures for this experiment only. Normal
Dolphin profiles and Emulia libraries are not migrated. Keep the printed profile
if it contains progress you want to inspect; subsequent probe launches do not
reuse it.

The bridge supplies an NSView inside SDL's Cocoa window to Dolphin's
WindowSystemInfo. Dolphin renders Metal directly into that view while egui draws
the surrounding shell with OpenGL. No screenshots or CPU pixel copies are used
for display. Emulia pumps Dolphin host jobs and stops/shuts down the core before
removing the view. The library remains loaded for process lifetime because
Dolphin has global objects and callbacks. Dolphin internals are pinned source
interfaces, not a stable third-party SDK.

See [qualification and limitations](../../docs/DOLPHIN-EMBEDDING-SPIKE.md).
