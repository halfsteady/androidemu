# Desktop shell: porting the Android UI to macOS and Linux

Date: 2026-09-10. Status: approved design, ready for an implementation plan.

## Goal

`nes-desktop` today is a 440-line SDL loop: one window, keyboard input, one
save slot, battery saves, no UI. The Android app has a library shelf, a pause
menu, ten save slots with thumbnails, a rewind/fast-forward scrubber, picture
settings with ten GPU looks and four palettes, a gamepad mapping wizard and
screenshots. This design brings all of that to the desktop player with the same
behaviour, wording and file layout, so a person who knows one shell knows the
other.

Out of scope: touch controls, signed macOS app bundles, sync between devices,
new emulation features.

## Approach

Keep SDL2 (bundled, static) for the window, high-DPI, audio queue, focus events
and game controllers. Add an OpenGL 3.3 core context from SDL, draw the picture
through the ported GLSL looks, and paint egui panels on top with `egui_glow`.
The SDL-to-egui bridge is our own code, not the `egui_sdl2_gl` crate.

Rejected: replacing SDL with eframe (rewrites the validated platform layer and
loses the dummy-driver smoke test); hand-rolled widgets on the SDL canvas (text,
layout and dialogs from scratch for a worse result).

## Process model

One main thread. Each loop iteration:

1. Poll SDL events. Translate them into egui `RawInput`, hotkeys, controller
   state, drag-and-drop paths, focus changes and the quit request.
2. Decide the emulation budget. Normal play: step a frame while the audio queue
   holds less than two frames of samples, exactly as today. Fast-forward at N×:
   step N frames per display tick, queue only the last frame's samples. Rewind
   at N×: pop N rewind states per display tick, clear the queue. Paused, or a
   panel open, or unfocused: step nothing, pause and clear audio, zero buttons.
3. Upload the RGB frame, run the preparation pass if the look needs one, draw the
   present pass into the letterboxed viewport, run the egui frame, paint it,
   swap. Vsync on (`gl_set_swap_interval(1)`) when the driver allows; otherwise
   sleep to the region's frame time as today.

Long jobs (import, box art decode, save with thumbnail, screenshot) run inline
with emulation paused and a busy scrim, matching Android's `work()`. Inline but
not immediately: the frame is built before the actions it raised are applied, so
a job that ran where it was asked for would run under a frame drawn without the
scrim. Raising the scrim holds the job in `App::pending`, and the iteration
after the one that painted that scrim runs it — a sixtieth of a second between
asking and doing, and no frozen window with nothing on it.

A separate emulation thread was rejected: SDL's `AudioQueue` is not `Send`, and
the single loop already keeps audio ahead of the display.

## Modules

All under `desktop/src/`. Everything except `shell.rs`, `video.rs` and `ui/`
must build and test without a window.

| Module | Owns |
|---|---|
| `main.rs` | CLI, headless mode, launching the shell |
| `engine.rs` | `Nes`, `Rewind` (64 MB), audio queue, scrub speed, pause, frame stepping, save/load state, battery RAM, reset that keeps battery RAM, header notes, the 64-entry active palette and the palette-applied 256×240 RGB frame |
| `library.rs` | index, per-game folder, import with dedupe, thumbnails, box art, archive, playtime, problem log |
| `settings.rs` | `settings.json`, `controllers.json`, migrations |
| `input.rs` | keyboard and SDL game controllers on two ports, mapping profiles, wizard capture, shoulder/trigger time control |
| `picture.rs` | geometry (shape, trim, integer scale), palette model, `.pal` parsing, sample frame |
| `scrub.rs` | scrub speed and label math |
| `video.rs` | glow pipeline: frame texture, scale2x and composite preparation passes, present shader, offscreen preview readback |
| `shaders/*.glsl` | `smooth.frag`, `composite.frag`, `present.frag`, `quad.vert` |
| `ui/theme.rs` | design tokens applied to `egui::Style` |
| `ui/shelf.rs` | shelf, cards, rows, empty state, game menu |
| `ui/play.rs` | title bar, fullscreen chrome, HUD notice |
| `ui/time.rs` | time scrubber and skip-back widgets |
| `ui/panels.rs` | pause, slots, settings, mapping, problem log, confirmations |
| `shell.rs` | SDL window, GL context, egui bridge, main loop, app state machine, the busy scrim and the message bar over whichever screen is up |

### CLI

```
nes-desktop [rom.nes] [--data-dir <dir>] [--mute] [--frames <n>] [--list-drivers] [--help]
```

- No ROM: open the shelf.
- ROM path: import into the library (copy, dedupe by hash) and open it.
- `--data-dir`: the data directory. `--save-dir` remains as an alias.
- `--frames <n>`: headless. Runs the engine for n frames with no window and no
  GL context, with the same audio, battery-save and autosave-failure behaviour
  the smoke test checks today. Prints the save location. Requires a ROM.
- `--mute`, `--list-drivers`, `--help`: unchanged.

## Data layout

Default directory unchanged: `~/Library/Application Support/Emulia` on macOS,
`$XDG_DATA_HOME/emulia` or `~/.local/share/emulia` on Linux.

```
settings.json
controllers.json
palette.pal                       the imported palette, raw bytes
library/index.json                [{id, title, added, played, seconds, archived}]
library/problems.log              last 200 lines, "yyyy-MM-dd HH:mm:ss  label — detail"
library/<hash>/game.nes           private copy, bounded to 16 MB on import
library/<hash>/battery.sav        shared by every slot
library/<hash>/auto.state  auto.png
library/<hash>/slot-0.state … slot-9.state, slot-0.png … slot-9.png
library/<hash>/art.png            chosen box art, longest edge ≤ 1024, PNG
```

`<hash>` is `format!("{:016x}", header.hash)`, the same key Android uses.
Timestamps are milliseconds since the epoch; a slot is empty when its state
file does not exist. Every write is atomic (temp file in the same directory,
fsync, rename) using the existing `write_save`.

**Migration.** The old player keyed files by `header.identity` at the directory
root: `<identity>.sav` and `<identity>.state`. When a game is opened and
`library/<hash>/battery.sav` does not exist, move `<identity>.sav` there; when
`slot-0.state` does not exist, move `<identity>.state` there. Log a problem-log
line saying what moved. Nothing is deleted otherwise.

**Screenshots** go to `~/Pictures/Emulia/<sanitised title> yyyy-MM-dd HH.mm.ss.png`.
On Linux, honour `XDG_PICTURES_DIR` from `~/.config/user-dirs.dirs` when set and
absolute, as `XDG_DATA_HOME` is.
Raw 256×240 framebuffer, no shape, trim or look, as on Android.

### settings.json

```json
{"aspect": 0, "look": 0, "palette": 0, "trimEdges": false, "shelfList": false, "fullscreen": false}
```

Ids match Android: `look` uses `Filter.id` (0 Off, 1 Scanlines, 2 Old TV,
3 Dot matrix, 4 Four greens, 6 Old photo, 7 Neon, 8 Cartoon, 9 Smooth,
10 Composite; retired 5 maps to 6), `palette` uses `Palette.id` (0 Standard,
1 Hardware, 2 Soft, 3 Vivid, 5 From a file; retired 4 maps to 0). Unknown keys
are ignored, missing keys take defaults. If `palette` is 5 but `palette.pal` is
missing or invalid, fall back to Standard and log it.

### controllers.json

```json
{"keyboard": {"name": "Keyboard", "a": "X", "b": "Z", "select": "Right Shift", "start": "Return"},
 "030000005e0400008e02000000000000": {"name": "Xbox Controller", "a": "b", "b": "a", "select": "back", "start": "start"}}
```

Keys are SDL joystick GUID strings; values are SDL scancode names for the
keyboard and SDL game-controller button names for controllers. SDL2 names the
face buttons `a`, `b`, `x` and `y` by position — `a` is the south button — so
the defaults written here are NES A on `b` and NES B on `a`. (SDL3's
`south`/`east` names are a different library's and do not appear on disk.) A
missing field is read as an empty one, so a profile short of a button loses
that button rather than the whole file. A saved profile beats the built-in
defaults.

## Library

Ported from `Library.kt` and the shelf code in `MainActivity.kt`.

- `games()` filters archived and sorts by `max(played, added)` descending;
  `archived()` the same for archived rows.
- Import: read the file bounded to 16 MB, `Nes::new` to validate and hash,
  title is the file name without extension, dedupe on hash returning the
  existing row, copy to `game.nes`, add to the index.
- `record(id, seconds)` re-reads the index before adding playtime and stamps
  `played`. Playtime is measured with `Instant`, never wall clock.
- Cover precedence: `art.png`, then `auto.png`, then the initial on the gradient
  (`0x3b5a43` to `0x21301f`).
- Box art: decode with `image` (png, jpeg), downscale so the longest edge is
  1024, encode PNG.
- Delete removes the whole game folder and the index row.
- `log_problem(label, detail)` appends and trims to 200 lines. Header
  corrections from `header.fixes.labels()` are logged, never shown.

## Shelf

Wordmark header "EMULIA" (or "Put away" for the archive), actions Add a game,
Settings, Put away (N) when any are archived, grid/list toggle when any games
exist; in archive mode a single Back to the shelf. Responsive: actions beside
the heading when the window is at least 820 logical pixels wide, otherwise a
wrapped row; card width 220 or 150 below 600. Empty state: the mark,
"A shelf full of possibilities", "Add a game file (.nes) from your computer to
begin." and "Games stay on this device. No account needed."

Cards: 4:3 cover, overflow menu top right, title on two lines, "Resume →" if
`auto.state` exists else "Ready to play →", playtime ("N min played" under an
hour, "H h M min played" above, nothing under a minute). Rows: 76-pixel cover,
title, subtitle joining the two with " · ".

Game menu: Choose box art (native image picker), Clear box art (only when
`art.png` exists), Put this away. Archived: Bring back, Delete (danger style)
with the confirmation "Delete <title>?" / "This removes the game, its battery
save and all ten of its save states from this computer. It cannot be undone."
and buttons Delete forever / Keep it.

Dropping a `.nes` file on the window imports and opens it. Dropping an image on
a running game's window sets its box art.

## Play view

**Opening a game**: load `game.nes`, restore `battery.sav` then `auto.state`;
if either fails, log the cause, show "This game had to start from an earlier
point." and open paused. Migrate old saves first (above).

**Title bar** (not fullscreen): title, Full screen, Menu. **Fullscreen** (SDL
desktop fullscreen): chrome pill top right with Menu and Exit full screen,
shown on any input and hidden after 5 s idle; the mouse cursor hides with it.
The `fullscreen` setting persists the choice.

**Time control**: the scrubber and the ↺5 / ↺15 buttons sit bottom centre,
240 logical pixels wide, in both modes, hidden with the chrome in fullscreen.
Behaviour from `TimeScrubber` and `Scrub.kt`: drag left rewinds, right
fast-forwards, `MAX = 8`, `DEAD_ZONE = 0.12`, speed `ceil(past × 8)` clamped
1..8; release springs to centre and resumes; a click on the handle without
drag pauses; the handle shows "▮▮" at rest and "N×" while scrubbing, amber
backwards and green forwards. Jump buttons are disabled when the rewind depth is
below `seconds × frame_rate`. The HUD notice pill shows "Rewinding N×",
"Fast-forward N×" or "That's as far back as this goes." for 2.2 s.

**Pause menu** "Take your time" / "Progress saves automatically when you pause
or leave.": Resume game; tiles Save states, Screenshot, Full screen or Exit
full screen, Settings; then Reset game and Back to your shelf.

**Reset**: confirmation "Reset game?" / "Restart <title> from the beginning.
In-game saves and manual save slots are kept. The current session and rewind
history will be replaced." Reset / Cancel. Builds a fresh `Nes`, carries battery
RAM across, clears rewind, overwrites the autosave, resumes; on autosave
failure stays paused with "Game reset, but its automatic save couldn't be
updated."

**Save states panel** "Save states" / "Ten slots, plus a separate automatic
save.": adaptive grid of ten tiles with 4:3 thumbnail, "Slot N", "Empty · ready
for a moment" or the local short date and time, Save and Load (Load disabled
when empty). Saving over an occupied slot asks "Replace slot N?" / "This
replaces the progress saved in this slot. Your other slots stay available."
with Replace save / Keep it. A save writes the state, the battery RAM if any,
and a PNG thumbnail. Load is immediate and shows "Save loaded. Press Resume
when you're ready." and resets the rewind chain.

**Autosave** (slot `auto`) on: opening the pause menu (also records playtime),
window focus loss, leaving to the shelf, quitting, and after a reset. Battery
RAM is also flushed every five seconds as today.

**Screenshot** does not pause; shows "Screenshot saved to <folder>."

**Quit** (window close, Cmd+Q) autosaves and flushes battery before exiting.

**Busy**: scrim and spinner while a job runs; input is cleared.

## Input

Ports: keyboard and the first controller share port 1; the second controller
is port 2. Connection order decides. A disconnect pauses with "Controller
disconnected. Your game is paused."

Keyboard defaults: arrows, X = A, Z = B, Return = Start, Right Shift = Select.
Hotkeys: Escape opens the pause menu, or closes the topmost panel (ladder:
problem log, settings, mapping cancel, slots, pause menu, then exit fullscreen);
Space also opens the pause menu, and Space or Escape in the pause menu resumes
(pausing and the menu are one state, as on Android); F11 toggles fullscreen; F5 and F8 quick-save and quick-load
slot 1 (F5 over an occupied slot replaces it without asking, as today); `.`
held is fast-forward 2×, Shift+`.` 6×; `,` held is rewind 2×, Shift+`,` 6×;
Backspace jumps back 5 s, Shift+Backspace 15 s. R no longer resets.

Controller defaults (SDL layout): south = B, east = A, Back = Select,
Start = Start, D-pad and left stick (dead zone 0.5) = directions; right
shoulder +2×, right trigger +6×, left shoulder −2×, left trigger −6×, held to
apply. Start on a paused game resumes, through the saved profile.

**Mapping wizard** from Settings → Controller buttons → Set up: pauses the game,
shows "Press A" at large size, ✓ progress, "Step N of 4", the device name once
known; order A, B, Select, Start. Only presses (no repeats) count; all four
must come from the device that pressed first; a button already used is
rejected. Escape and the Cancel button abort. Completion saves the profile and
reports "Buttons saved for <name>. The directional pad and stick work
automatically."

## Picture

**Geometry** from `Picture.kt`: 256×240, `TRIM = 8` rows top and bottom when
trimming, pixel aspect 8/7. Television fits 4:3, Hardware fits
`256 × 8/7 : visible rows`, Pixel-perfect uses the largest integer scale of
`min(w/256, h/visible)`, at least 1. Centred, black outside. Uses
`drawable_size` so HiDPI is sharp.

**Palette** is applied on the CPU into the RGB frame, one 64-entry table.
Standard is the core's table (`palette.rs` today). Hardware, Soft and Vivid come
from the ported `PaletteModel` (2C02 square-wave generator, YIQ demodulation,
`build(saturation, tint, contrast, brightness, gamma)` with the Kotlin presets).
From a file parses `.pal` (at least 192 bytes, extra ignored) and offers
Replace. Choosing From a file with nothing imported opens the picker directly.
A palette change repaints the paused frame immediately.

**Looks** are the three Android programs ported to `#version 330 core` with
the same constants and comments: `smooth.frag` (scale2x twice, `texelFetch`,
mipmapped last level), `composite.frag` (`SUBSAMPLES = 4`, 24 taps, phase
walks a third of a cycle per line and per frame with `frames % 3`) and
`present.frag` (branches on `kind`: Scanlines, Old TV, Dot matrix, Four greens,
Old photo, Neon, Cartoon; Off, Smooth and Composite draw the prepared texture
as is). Each look's note text is shown under the chips.

**Settings panel**: preview at the top rendered through the real pipeline into
a 640×480 framebuffer object and shown as an egui texture. Sample source: the
paused frame if a game is open, else the newest autosave thumbnail on the
shelf, else the generated sample frame; always the generated frame when a
palette other than Standard is chosen. Sections: Shape, Look, Colours, Palette
file (Replace, only for From a file), Trim the edges, Controller buttons (Set
up), Audio delay (queued milliseconds from the SDL queue, polled twice a
second), Version, Problem log (Open). Done closes it and frees the preview.

**Problem log**: newest first, up to 40 rows, "Nothing has gone wrong yet."
when empty.

### Glyphs

egui's bundled proportional font has no glyph for several of the marks the
Android shell draws, and a missing glyph is a tofu box on the one control that
was meant to explain itself. Each is substituted for one the font does have,
and a test per module (`ui/widgets.rs`, `ui/shelf.rs`, `ui/time.rs`,
`ui/panels.rs`, `ui/settings.rs`) keeps the originals from creeping back in:

| Android | Desktop | Where |
|---|---|---|
| `→` | `›` | "Resume ›" on a shelf card |
| `⋯` | `…` | a card's overflow menu button |
| `←` | `‹` | the back arrow on a panel |
| `✓` | `•` | a learned button in the mapping wizard |
| `▮▮` | `⏸` | the time handle at rest |
| `◀◀ ▶▶` | `« »` | the scrubber's ends |

## Theme

`ui/theme.rs` applies the Android tokens to egui: background `#111813`,
surface `#1d2820`, raised surface `#243128`, text `#edf4e9`, outline `#6d7f68`,
primary `#b9e38c` on `#17300c`, raised `#2a3a2e` on `#c3d1bd`, error
`#ffb4a6` on `#5f1409`, amber `#f0d49a` on `#152210` for backwards time and
notices, well `#141d17`, chrome `#0a120c` at 80%, scrim at 85%. Radii 14, 20
and 28 logical pixels for rows, tiles and panels. Panel max width 620. Buttons
60, 52 and 46 tall for primary, secondary and quiet. Danger styling only
through the quiet action's danger flag. Section labels uppercase, small, bold,
letter-spaced, in primary green. egui's default fonts are used.

## Errors

Every failure goes through one `report(label, detail)` that writes the detail
to the problem log and shows the plain label in a dismissable message. Save
directory errors never close the game. Malformed files are reported and left
intact. A missing GL 3.3 context is a startup error naming the driver.

## Testing

Unit tests (no window):

- `scrub.rs`: the cases in `ScrubTest.kt`.
- `picture.rs`: geometry cases from `PictureTest.kt`; palette presets against
  the values in `PaletteTest.kt`; `.pal` parsing bounds; sample frame indices.
- `library.rs`: index round trip, dedupe on import, ordering, playtime record,
  archive and delete, problem log cap, cover precedence.
- `settings.rs`: defaults, retired id mapping, missing palette fallback,
  controller profile round trip.
- `input.rs`: default and saved profiles, wizard rejection rules.
- `engine.rs`: headless save and load, reset keeps battery RAM, autosave
  failure keeps the machine, save migration from the flat layout, the existing
  focus-flush and atomic-save tests.

Integration:

- `scripts/check-desktop.py`: unchanged persistence checks through headless
  `--frames`, plus a check that a ROM path creates `library/<hash>/game.nes`
  and an index row, and that a second run dedupes.
- `scripts/check-shaders.py`: also compiles `desktop/src/shaders/*` with
  `glslangValidator` when available.
- `cargo clippy -p nes-desktop --all-targets -- -D warnings` stays clean.

Manual, recorded in `docs/DESKTOP.md`: window and fullscreen, sound, each look,
a controller and the wizard, drag-and-drop, screenshots.

## Documentation

Rewrite `docs/DESKTOP.md` for the shell: build, CLI, keys, controllers, data
layout and migration, verification. Update the README's desktop row and the
sentence in the desktop doc that lists what desktop lacks.
