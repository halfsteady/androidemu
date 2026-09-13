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
latter. In this mode GameCube/Wii shelf launches also use the embedded host.
A feature-enabled app also discovers a backend bundled in `Contents/Frameworks`
when launched from Finder. Use `--release --desktop-app OUTPUT` to package a local
`Emulia.app`; see `docs/DESKTOP-PACKAGING.md`.
Default builds and packages retain the external launcher. Close this probe app
before rebuilding it; the builder refuses to overwrite a running executable.

Menu or Escape captures one frame, pauses Dolphin, hides its Metal view, and
shows the existing Emulia pause panel over that image. Resume restores the view.
The shared ten-slot screen supports save, replace confirmation and load. Pausing
and leaving write a separate autosave; reopening offers that moment in the pause
panel. Screenshot exports the pause picture. Reset restarts without restoring
the autosave. Rewind and fast-forward are intentionally absent for Dolphin.

Settings opens player-one control mappings. Keyboard and the first connected SDL
gamepad share that port. Buttons, each stick direction, analog triggers, pointer,
tilt and shake can be mapped. Escape cancels capture; Clear removes a binding.
GameCube uses its own layout. Wii games can choose GameCube (where the game
supports it), sideways Remote, Remote + Nunchuk, or Classic controller. Changing
controller style takes effect after Reset/reopening; remapping individual inputs
takes effect on Resume. Mappings are per game, saved atomically in
`library/<game-id>/dolphin-controls.json`.

GameCube defaults: X/Z = A/B, C/V = X/Y, F = Z, Return = Start,
WASD = main stick, IJKL = C stick, arrows = D-pad, Q/E = triggers.
Wii uses X/Z = A/B, C/V = 1/2, Return = +, Space = shake;
IJKL/right stick aim the pointer. All displayed bindings can be changed.
New Wii entries default to Remote + Nunchuk. Mouse movement aims the Wii Remote;
left click is A, right click is B, including the selected extension's buttons.
Dolphin's renderer-provided input scale aligns aiming with the displayed picture,
including letterboxing and pillarboxing. Only clicks inside the focused game
picture are forwarded. The build runs standalone pointer-geometry regression tests.
Mouse alignment in Settings offers independent horizontal/vertical range controls;
lower a value if the in-game pointer travels too far. USA Mario Kart Wii uses the
measured 0.75/0.96 defaults. Overrides persist per game and apply on Resume,
without changing gamepad input. Reset mouse alignment restores the game's defaults. Moving a stick switches
back to gamepad aiming until the mouse moves; Settings can disable mouse input.
Nunchuk C uses Left Shift, avoiding the WASD stick bindings. The sideways preset
uses WASD/left stick for tilt; other styles leave tilt unbound by default.
Physical controller feel, rumble and motion-heavy games are not qualified by
the automated checks; rumble and additional player ports are not implemented.

The owned Dolphin profile now persists under
`<data-dir>/dolphin/embedded-2606a`, protected by an OS file lock. It contains
Dolphin's memory cards/NAND and controller expressions for Emulia's virtual
input device. It initializes Metal, disables frame dumping/counters/analytics,
and acknowledges NKit fixtures for this experiment only. Normal Dolphin
profiles and older temporary probe profiles are not migrated.

The bridge supplies an NSView inside SDL's Cocoa window to Dolphin's
WindowSystemInfo. Metal renders directly while playing; a single PNG readback
is used only when entering the pause overlay. Dolphin remains loaded for process
lifetime because its globals retain callbacks. Shutdown services the AppKit
queue before joining the emulation thread, including during boot cancellation.

`patch_state.py` applies a small checked-operation patch to pinned Dolphin
`State.h`/`State.cpp`. It retains Dolphin's state format and reports serialization,
write, close, rename and load failure. Emulia writes to staging and atomically
replaces a slot only after success. The builder rejects unrelated source edits.
These are pinned internal interfaces, not a stable Dolphin SDK.

Run the reproducible debug-build qualification with a **new disposable** directory:

```sh
python3 scripts/dolphin-spike/qualify.py \
  --app target/dolphin-spike/EmuliaDolphinProbe.app \
  --game '/path/to/Animal Crossing (USA).iso' \
  --work-dir /tmp/emulia-gc-qualification
# Use --wii and a Wii disc to check the three Wii controller styles.
```

The harness checks state round trips, failed operations, thumbnails, virtual
controller values, mapping persistence/restarts and shutdown. An explicit
`EMULIA_DOLPHIN_PROBE_COMMAND` file enables its in-app command/status interface
in debug builds only; it does not generate macOS input events. It is not a
replacement for physical gamepad or sustained-play qualification. The build
optimizes the host and SHA-256 even in debug mode so large Wii images can be verified promptly.

See [qualification and limitations](../../docs/DOLPHIN-EMBEDDING-SPIKE.md).
