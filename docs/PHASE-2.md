# Phase 2 — Amelia

The phase that makes the tablet hers. It landed without the locked-down kid mode
the plan originally called for: that was built, and then removed on the grounds
that Amelia is trusted with the whole app. What the mode was for — a shelf of box
art, one tap back into a game, nothing sharp within reach — is now simply how the
app behaves, with the depth one panel away rather than behind a code.

## Implemented

- **The shelf.** Box-art tiles ordered by what was played most recently, falling
  back to when a game was added. One tap loads the ROM, restores the battery save
  and the automatic save, and resumes — no second step between the tile and a
  running game. Playtime shows on the tile once there is a minute of it.
- **Box art.** A picture chosen per game outranks the last saved screenshot, which
  outranks a tinted initial. Imported through the storage picker, downscaled to a
  1024 px long edge on the way in, written atomically as `art.png` beside the game.
  Clearing it falls back to the saved moment rather than blanking the tile.
- **One time control.** Rewind and fast-forward were a pair of hold buttons; they
  are now a single track, because they were always one axis. Drag left of centre to
  run the game backwards, right to run it forward, and the further from centre the
  faster it goes — up to 8× either way. Let go and the thumb springs back to the
  middle and play resumes. The centre is a dead zone, so a thumb resting slightly
  off centre does not creep the game along, and the thumb shows the current
  multiplier. `Scrub.kt` holds the mapping from finger position to speed as a pure
  function, because that is the part that has to feel right.
- **Fixed jumps back**, 5 s and 15 s, beside the track: one tap, a known distance,
  no holding. Dimmed and inert when the rewind chain is shorter than the jump —
  offering to undo fifteen seconds that were never recorded is a promise the buffer
  cannot keep. The chain depth is polled while a game is open to decide that.
- **Shoulder buttons** drive the same control, with the same "further is faster"
  idea: bumpers run 2×, triggers 6×, in whichever direction. They sit outside the
  mapping wizard's four buttons deliberately.
- **Picture settings**, previewed and applied live:
  - Shape: 4:3 television, 8:7 hardware (the 2C02's real pixel aspect), or
    pixel-perfect — whole-number scaling only, which leaves a wider border rather
    than resampling a NES pixel into an uneven number of screen pixels.
  - Trim the edges: hides the 8 rows top and bottom that a television lost to
    overscan, and that so many games fill with seams and scroll garbage.
  - Eight looks: off, scanlines, old TV (scanlines, a curved tube, an aperture
    grille counted in device pixels, and a darkened edge), dot matrix, four greens,
    black and white, old photo, and neon. All one fragment shader with a branch, so
    switching costs nothing and there is one place to read.
- **A real preview.** The settings panel shows the current shape, trim and look on
  a still — and it is not a mock-up. `ScreenRenderer` draws it into an off-screen
  buffer with the same shader the game uses and reads the pixels back, so the
  preview cannot drift from the result. Mid-game it previews the frame you paused
  on; from the shelf it uses the newest saved moment, or a built pattern (colour
  bars, a ramp, a one-pixel checkerboard and bright blocks) chosen so every filter
  has something to act on. The preview box is 4:3, so a narrower shape shows its
  own side bars — which is the difference worth seeing.
- **Large touch targets sized from the screen** rather than from a setting. A 13"
  tablet in landscape gets about 1.45× the base size, a narrow window gets 1×.
  The "big controls" switch is gone; so is the fast-forward speed setting, which
  the track's distance-from-centre replaced.
- **Screenshots** to the device's own `Pictures/Emulia`, so they appear in the
  gallery. No permission is needed for a collection this app wrote itself. It runs
  on the GL thread to stay ordered with save and rewind, and does not pause the
  game. The framebuffer is what lands in the file, so shape, trim and look are not
  baked in — that is the picture the console produced.
- **Plain failures, real reasons kept.** One sentence to the player; the underlying
  message goes to a bounded `problems.log` under `files/library/`, capped at 200
  lines, readable from Settings. The dialog shows both.
- **Autosave on a low battery**, in addition to pause and background. A registered
  receiver for `ACTION_BATTERY_LOW` writes through the same atomic path, so
  progress survives the tablet dying unattended.

## Verification

- Host JVM, 20 tests passing:
  - `ScrubTest` over the time control's feel — a dead centre, clamped ends, speed
    that rises monotonically with distance and never skips backwards, left as an
    exact mirror of right, and every step from 1 to 8 actually reachable.
  - `PictureTest` over the presentation geometry — both aspects, the trim fraction,
    whole-number-only pixel-perfect scaling, centring and containment across a
    sweep of surface sizes, and a degenerate surface drawing nothing.
  - `LookTest` pins the filter ordinals (they are the persisted values, so
    appending is fine and reordering would silently change everybody's saved look),
    checks an unknown ordinal falls back rather than crashing, and asserts the
    built sample pattern actually contains colour, a ramp, alternating rows and
    bright blocks — a flat sample would preview nothing.
  - The existing JNI bridge and rewind-chain tests.
- **The shaders are compile-checked**, by `scripts/check-shaders.py`: it pulls both
  GLSL sources out of the Kotlin string literal and runs them through
  `glslangValidator`. This is not decoration — it catches real errors, including
  the one this work shipped into the first draft (`sample` is a reserved word in
  GLSL ES 3.00, and the shader would have failed to compile to a black screen on
  the tablet with the Kotlin compiling perfectly happily). It skips when no
  validator is present, the way `check-roms.py` skips without its ROMs.
- Instrumentation (written; needs a device or a working emulator):
  `playPauseSaveLoadAndResume`, `pictureSettingsPreviewAndPersist` (waits for the
  real preview to render, then drives the chips), `anOldScanlinesSwitchBecomesTheScanlinesLook`,
  `skipBackIsOfferedOnlyWhenThereIsSomethingToGoBackTo`,
  `chosenBoxArtOutranksTheSavedScreenshot` and `playtimeAccumulatesAndOrdersTheShelf`.
- `assembleDebug`, `lintDebug` (0 errors) and `testDebugUnitTest` pass offline.

## Not verified here

The shaders compile and the geometry is unit-tested, but **nobody has looked at
the eight looks on the panel**. Whether the aperture grille reads as a CRT or as
shimmer at this scale, whether the curvature is pleasant or seasick, and whether
the neon bloom is fun or garish are all judgements that need eyes on hardware.
The cost of 8× fast-forward and of an 8× rewind is also unmeasured.

Settings written by an older build are migrated, not reset: the scanlines switch
the first release shipped becomes the scanlines look, and that path has a test.

## Remaining

- [ ] Look at the eight looks on the tablet and cut the ones that do not earn
  their place. Measure 8× in both directions.
- [ ] The accurate end of §3 — the NTSC composite filter, the CRT shader ports,
  xBRZ — is real work and is not pretending to be here.
- [ ] Per-game picture overrides. Settings are global today.
- [ ] Slow motion and frame advance, the other half of the time controls.
- [ ] Box-art packs keyed by ROM hash (§8; needs a metadata pack that doesn't exist).
- [ ] Favourites, and a portrait tile shape for chosen art — the tile is 4:3
  because a saved screenshot is.
- [ ] Backup export of saves, states and settings (§8).
