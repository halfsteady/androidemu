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
- **Fast-forward**, held rather than toggled, beside rewind. It runs 2×, 4× or 8×
  emulated frames per displayed frame; the held input carries through every frame
  in the batch, so holding right through a fast-forward behaves the way it looks.
  Audio stops rather than playing back at four times the pitch, which also keeps
  the sample queue from overrunning. Rewind and fast-forward refuse to run at once.
- **Shoulder buttons** drive both time controls (R1/R2 forward, L1/L2 back), so a
  controller session never has to reach for the screen to skip a cutscene or undo
  a fall. They are deliberately outside the mapping wizard's four buttons.
- **Picture settings**, applied live so their effect is visible on the game behind
  the panel:
  - Shape: 4:3 television, 8:7 hardware (the 2C02's real pixel aspect), or
    pixel-perfect — whole-number scaling only, which leaves a wider border rather
    than resampling a NES pixel into an uneven number of screen pixels.
  - Trim the edges: hides the 8 rows top and bottom that a television lost to
    overscan, and that so many games fill with seams and scroll garbage.
  - Scanlines: one soft dark band per source row, computed from the texture
    coordinate so it doesn't alias into moiré at fractional scales.
- **Large touch targets**, with a "big controls" setting that scales them a further
  1.4× (1.15× on narrow widths, where a bigger layout would overflow).
- **Screenshots** to the device's own `Pictures/Emulia`, so they appear in the
  gallery. No permission is needed for a collection this app wrote itself. It runs
  on the GL thread to stay ordered with save and rewind, and does not pause the
  game. The framebuffer is what lands in the file, so shape, trim and scanlines are
  not baked in — that is the picture the console produced.
- **Plain failures, real reasons kept.** One sentence to the player; the underlying
  message goes to a bounded `problems.log` under `files/library/`, capped at 200
  lines, readable from Settings. The dialog shows both.
- **Autosave on a low battery**, in addition to pause and background. A registered
  receiver for `ACTION_BATTERY_LOW` writes through the same atomic path, so
  progress survives the tablet dying unattended.
- **One settings panel**, reachable from the shelf and from a paused game.

## Verification

- Host JVM, 10 tests passing: the existing JNI bridge and rewind-chain tests, plus
  `PictureTest` over the presentation geometry — 4:3 and 8:7 aspects, the trim
  fraction, whole-number-only pixel-perfect scaling, centring and containment
  across a sweep of surface sizes, and a degenerate surface drawing nothing. The
  geometry is deliberately a pure function in `Picture.kt` rather than inline GL
  code, so it can be checked without a device.
- Instrumentation (written; needs a device or a working emulator):
  `playPauseSaveLoadAndResume` covers the play → pause → save → load → background →
  resume flow, `settingsCycleAndPersist` drives the panel and asserts a fresh
  reader sees what it wrote, `chosenBoxArtOutranksTheSavedScreenshot` covers cover
  precedence, and `playtimeAccumulatesAndOrdersTheShelf` covers the index.
- `assembleDebug`, `lintDebug` (0 errors) and `testDebugUnitTest` pass offline.

## Not verified here

The scanline shader, the viewport changes and the aspect modes are compile-checked
only. The geometry behind them is unit-tested, but nobody has looked at the result
on the panel — that needs the tablet, and so does the cost of fast-forward at 8×.

## Remaining

- [ ] Look at each picture mode on the tablet, and measure whether 8× fast-forward
  holds its frame rate.
- [ ] Box-art packs keyed by ROM hash, so identified games arrive with art instead
  of needing a picture chosen by hand (§8; needs a metadata pack that doesn't exist).
- [ ] Per-game picture overrides. Settings are global today.
- [ ] Favourites, and a portrait box-art tile shape for chosen art — the tile is
  4:3 because a saved screenshot is.
- [ ] Backup export of saves, states and settings (§8).
- [ ] Slow motion and frame advance, which are the other half of the time controls
  and belong with the speedrun work in §4.
