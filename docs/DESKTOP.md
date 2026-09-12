# Emulia on macOS and Linux

The emulator was already portable: `nes-core` has no dependencies or platform
I/O, and `nes-runner` uses Rust's standard library. CPU, PPU, APU, cartridge
mappers, input registers, rewind and state serialization all live in `core/`.
The Android JNI and AAudio integration lives separately in `native/`.

`nes-desktop` is the whole shell on that core, in SDL2, OpenGL and egui: the
box-art shelf with import, archive and playtime; a letterboxed 256×240 picture
at 48 kHz with the same ten looks, the same palettes and the same `.pal` import
Android has; the draggable time control for rewind and fast-forward; the pause
menu with ten save slots, thumbnails, screenshots and a readable problem log;
settings previewed by the real shader; and two controller ports with a mapping
wizard. The saves and the folder layout are Android's, file for file. Colour
emphasis remains a core limitation.

## Build

Use current stable Rust, a C/C++ compiler and CMake. No Android SDK, NDK, Java or
separately installed SDL is needed. SDL2 is built from the locked dependency's
sources and statically linked (see [Rust-SDL2's build documentation](https://github.com/Rust-SDL2/rust-sdl2)).
The repository's Cargo configuration supplies compatibility policies for CMake 4
and a macOS 11 deployment target; either environment default can be overridden.

On macOS, install Xcode Command Line Tools (`xcode-select --install`) and CMake
(`brew install cmake` if using Homebrew). Both Apple Silicon and Intel builds
are covered by the CI workflow.

On Ubuntu/Debian, install the build and desktop development packages:

```sh
sudo apt-get install build-essential cmake pkg-config libx11-dev libxext-dev libxrandr-dev libxcursor-dev libxi-dev libxfixes-dev libxss-dev libasound2-dev libpulse-dev libwayland-dev libxkbcommon-dev libegl1-mesa-dev libgl1-mesa-dev
```

From the repository root:

```sh
cargo build --release --locked -p nes-desktop -p nes-runner
./target/release/nes-desktop "/path/to/game.nes"
```

SDL can build successfully with only dummy/offscreen drivers when development
packages are missing. Check `nes-desktop --list-drivers`: Linux needs `x11` or
`wayland` for the window and a desktop audio driver such as `alsa` or
`pulseaudio`; macOS needs `cocoa` and `coreaudio`. The CI smoke test checks these
compiled backends as well as exercising the dummy drivers.

"Add a game" and "Choose box art" open the system file chooser. On Linux that is
the XDG desktop portal, spoken over D-Bus, so it needs no extra build packages —
but it does need `xdg-desktop-portal` running with a backend for the desktop in
use (`xdg-desktop-portal-gtk`, `-kde` or `-wlr`). Without one, dragging a `.nes`
file or a picture onto the window does the same job.

The executable can be copied out of the repository and run directly. Linux
still needs the system C runtime and display/audio services; it is not a fully
static executable for arbitrary distributions. CI builds on Ubuntu 22.04 for
x86-64 and ARM64, and macOS 15 for Apple Silicon and Intel. Build from source on
older systems. The workflow uploads tar archives containing the two executables
and these instructions. These are terminal-launched binaries, not signed macOS
application bundles. No ROMs are included.

For core/CLI work alone, only Rust is needed:

```sh
cargo test --locked -p nes-core -p nes-runner
cargo run --release -p nes-runner -- frames "/path/to/game.nes" 60
```

The default workspace members remain core, runner and Android bridge, so a bare
`cargo build` does not require desktop dependencies. `--workspace` includes SDL.

## Shelf and play

With no arguments the shelf opens. A ROM path on the command line, or a `.nes`
file dropped on the window, is copied into the library and opened; the copy is
keyed by the ROM's payload hash, so adding the same game twice adds it once.
"Add a game" opens the system file chooser for the same thing.

Each card carries its box art, or the newest autosave thumbnail when there is
none. The card menu chooses and clears box art, and dropping a `.png`, `.jpg`
or `.jpeg` on the window while a game is open sets that game's art. "Put this
away" moves a game to the archive, which "Put away (n)" opens; from there it
can be brought back or deleted, and Escape comes back to the shelf. Deleting
removes the game's folder — the ROM copy, the battery save and all ten states.
The Cards/List toggle is remembered. Covers are decoded as they are scrolled
to rather than all at once, so a long shelf opens as quickly as a short one,
and the ones scrolled past are let go of again.

Importing a game, turning a picture into box art, saving a slot with its
thumbnail and writing a screenshot all happen on the one thread that draws the
window. Each raises a dimming scrim with a spinner, over the shelf or over the
game, and runs on the frame after that scrim has been painted; the game is
paused and no button reaches it while it is up. Several at once — a handful of
files dropped together — queue behind the scrim and run one at a time, in the
order they were asked for, so the last game dropped is the one left open. A
confirmation comes down with the click that answers it rather than staying up
over the work it asked for. The pointer takes the shape of
whatever is under it — a hand over a control, an I-beam over text.

In a game, the Menu button, Escape and Space all open the pause menu: Resume
game, Save states, Screenshot, Full screen, Settings, Reset game and Back to
your shelf. Clicking away to another window opens it too, so walking away
costs nothing; Settings → Play turns that off for somebody who wants a game
running in a window behind something else, and with it off the held keys are
still let go of — a key released while another window has the keyboard is
never reported — and the battery RAM is still written, but nothing stops and
no automatic save is taken. Escape backs out one level at a time. F11 is full
screen; after five seconds with no key, no mouse, no controller button, no
stick or trigger past its dead zone and no touch of the time control the
chrome fades, and any of those brings it back.

```
nes-desktop [rom.nes] [--data-dir <dir>] [--mute] [--frames <n>] [--list-drivers] [--help]
```

Sound clocks the emulation, independently of the display's refresh rate.
`--mute` plays without it, pacing on the ROM region's frame rate and a
monotonic clock instead; use it where there is no audio device. The window
itself is paced by vsync when the driver gives it one, and otherwise sleeps out
the rest of each frame — the region's when a game is open, a sixtieth of a
second on the shelf. "vsync unavailable; pacing by sleep" on the terminal is
that second case. `--frames <n>` needs a ROM and runs that many frames with no
window and no GL context at all, printing where the data went — it is what CI
runs under SDL's dummy drivers.
Audio and the battery behave as they do in the window, and the run starts from
the autosave as usual, refusing to carry on if it cannot be read; it writes no
new one, because with nothing on screen there is nobody to decide whether
losing that progress is acceptable. `--list-drivers` prints the compiled SDL
video and audio backends.

## Keys and controllers

| Key | Action |
|---|---|
| Arrow keys | Direction pad (remappable, unlike a controller's) |
| X / Z | A / B |
| Enter / Right Shift | Start / Select |
| Escape or Space | Pause menu (Escape also backs out of panels, and of the archive) |
| F11 | Full screen |
| F5 / F8 | Save / load slot 1 |
| . / , (hold) | Fast-forward / rewind 2×, Shift for 6× |
| Backspace | Jump back 5 s, Shift for 15 s |

Any SDL game controller works without configuration. The south face button is
NES B and the east one NES A, because a thumb rests on the south button and
the other way round makes every game feel backwards; Back is Select and Start
is Start. The direction pad and the left stick both steer. The shoulders run
time at 2× and the triggers at 6×, left for backwards and right for forwards,
held rather than toggled. Two controllers are taken, in the order they are
plugged in, as the two ports.

Settings → Controller buttons → Set up remaps the buttons, either for a
controller or for the keyboard, and whichever device presses first owns the
rest of the steps. A controller is asked for four — A, B, Select and Start —
because its direction pad and its left stick steer on their own and are not
remapped. The keyboard is asked for eight: those four, and then Up, Down, Left
and Right, since a keyboard has no direction pad to fall back on. Press them in
turn and the profile is saved under that controller's SDL GUID, or under
`keyboard`. Escape cancels.

Any key can be mapped, Tab and the arrows included: the keyboard reaches the
game and the hotkeys and nothing else, and egui's own focus and zoom shortcuts
are off, so a key never moves the panels' focus or resizes the screen out from
under the picture. Five keys are the shell's own — Space, Backspace, F5, F8
and F11 — and the wizard refuses them with "That key already does something.
Pick another.", because the hotkey would swallow the button before the game
ever saw it; the next press that is not one of them puts that sentence away.
Escape cancels the wizard rather than being refused by it, as it backs out of
everything else. `.` and `,` are held rather than pressed and can be mapped.

## Saves and data

The data directory is `~/Library/Application Support/Emulia` on macOS and
`$XDG_DATA_HOME/emulia` (or `~/.local/share/emulia`) on Linux. Pass
`--data-dir /path/to/dir` to override it, including a directory on removable
storage; `--save-dir` is still accepted as an alias for it.

```
settings.json                     shape, look, palette, trim, shelf view,
                                  full screen, pause on focus loss
controllers.json                  one button profile per SDL GUID, plus "keyboard"
palette.pal                       the imported palette, raw bytes
library/index.json                one entry per game: id, title, added, played, seconds, archived
library/problems.log              the last 200 lines of what went wrong, and why
library/<hash>/game.nes           the private copy, bounded to 16 MB on import
library/<hash>/battery.sav        the battery RAM, shared by every slot
library/<hash>/auto.state         the automatic save, with auto.png beside it
library/<hash>/slot-0.state …     the ten slots (slot 1 is slot-0), each with
library/<hash>/slot-9.state       a slot-N.png thumbnail beside it
library/<hash>/art.png            chosen box art, longest edge at most 1024
```

`<hash>` is a hash of the ROM's payload rather than of its file name, and it is
the key Android uses too, so moving or renaming a game file keeps its saves.
Every write is atomic — a temp file in the same directory, fsync, rename — so a
crash never leaves half a save, index or setting behind.

Battery RAM is written every five seconds while a game runs, the moment it is
paused (losing the window's focus pauses it unless Settings → Play says
otherwise, in which case the focus loss writes it without pausing), whenever a
slot is saved, on the way back to the shelf and on quit. Leaving and quitting
also write the automatic save, which is what the game resumes from next time.
Force-quitting can still lose the few seconds since the last flush.

A battery save that will not load is never written over: it is renamed
`battery.sav.unreadable` beside itself, the reason goes in the problem log, and
the game opens paused saying it had to start from an earlier point. If it
cannot even be renamed, the session writes no battery at all rather than
replacing someone's adventure with empty RAM. An autosave that will not load is
reported the same way and the game starts from the battery instead.

A `settings.json` or `controllers.json` that will not parse is never written
over: the shell runs on the usual choices, says so, and names the file in the
problem log, so a stray comma can be fixed by hand. Nothing saves either file
again until a setting is changed or the mapping wizard finishes. A profile
missing one of its buttons loses that button and keeps the rest of the file; a
keyboard profile with no directions saved steers by the arrow keys, which is
how every profile written before they could be remapped still plays. `palette.pal` is the same: `palette` 5 with the file missing or no longer
a palette paints in the standard colours and says which file it was.

The first desktop player kept `<identity>.sav` and `<identity>.state` loose in
the data directory. Opening a game moves them once into its folder, as
`battery.sav` and slot 1, and says so in the problem log; nothing is deleted.
Android save containers and their metadata are not imported.

Screenshots are the raw 256×240 frame with no shape, trim or look, written to
`~/Pictures/Emulia/<title> yyyy-MM-dd HH.mm.ss.png`. On Linux a
`XDG_PICTURES_DIR` set in `~/.config/user-dirs.dirs` is honoured when it names
an absolute directory; a relative one is ignored in favour of `~/Pictures`.

Avoid running two copies of the same game against the same data directory: the
last writer wins.

## Verification

```sh
cargo fmt -p nes-desktop -- --check
cargo test --workspace --locked
cargo clippy --locked -p nes-core -p nes-runner -p nes-desktop --all-targets --no-deps -- -D warnings
cargo build --release --locked -p nes-desktop -p nes-runner
python3 scripts/check-desktop.py target/release/nes-desktop --require-native-drivers
python3 scripts/check-shaders.py
```

`cargo fmt` is checked for `nes-desktop` alone; `core/` and `native/` predate it
and are not rustfmt-clean. Clippy is green for all three crates, on the clippy
CI runs and on stable 1.95.

The smoke test creates its own NROM, runs the real executable headlessly
through `--frames` with SDL's dummy video and audio drivers, and checks audio
pacing, muted pacing, the library import and its deduplication, SRAM
persistence, autosave recovery and invalid inputs. CI runs it on Ubuntu 22.04
and macOS 15, x86-64 and ARM64. `check-shaders.py` compiles the desktop GLSL in
`desktop/src/shaders` as well as Android's, using `glslangValidator` from
`PATH` or the Android SDK's emulator, and skips when neither is present.

Dummy drivers cannot verify a window, physical keys, a controller, audible
sound or GPU presentation. Those need a person, a ROM they own and one
controller. Working through the shell:

- [ ] The game runs in the letterboxed area with sound, and the arrow keys,
      X, Z, Enter and Right Shift play it.
- [ ] Space freezes the picture and stops the sound; F11 is full screen; F5
      then F8 restores; `.` fast-forwards, `,` rewinds and Backspace jumps
      back.
- [ ] Closing the window writes `library/<hash>/auto.state` and `battery.sav`.
- [ ] A bad ROM path shows "That game file didn't work." with an OK button
      that dismisses it.
- [ ] The shelf shows the game with its autosave thumbnail as the cover,
      "Resume ›" and its playtime.
- [ ] "Add a game" opens a native file chooser and imports; a second import of
      the same file does not add a second card.
- [ ] The card menu sets and clears box art, and dragging an image onto the
      window while a game is open sets it.
- [ ] "Put this away" moves a game to the archive; "Put away (1)" shows it
      with "Bring back" and "Delete"; Escape comes back to the shelf; clicking
      a card opens the game.
- [ ] Importing a game and choosing box art dim the screen behind a spinner
      rather than freezing the window, and the pointer is a hand over the
      shelf's buttons and an arrow over its background.
- [ ] The Cards/List toggle survives a restart.
- [ ] Dragging the time handle left rewinds, with the handle amber under an
      amber "Rewinding N×" pill; dragging right turns the handle green under
      an amber "Fast-forward N×"; releasing springs it back and play resumes.
- [ ] ↺5 and ↺15 are dim until there is history to jump back into, and jump
      back when there is; at the end of the tape "That's as far back as this
      goes." shows for about two seconds.
- [ ] In full screen the chrome hides after five idle seconds, and a mouse
      move brings back the pill and the time row.
- [ ] The pause menu says "Take your time"; "Save states" shows ten tiles;
      saving into an empty slot 1 is immediate and saving again asks "Replace
      slot 1?".
- [ ] Load restores and says "Save loaded. Press Resume when you're ready."
- [ ] "Screenshot" writes into `~/Pictures/Emulia` and says so.
- [ ] "Reset game" asks first and restarts with the battery save kept.
- [ ] "Back to your shelf" returns with the autosave thumbnail as the cover,
      and Escape backs out one level at a time.
- [ ] Start on a controller resumes from the pause panel.
- [ ] From the shelf, Settings previews the newest thumbnail or the built
      pattern; every Look changes it through the real shader; Shape and Trim
      change the letterbox; Colours restains it.
- [ ] "From a file" opens the picker: a valid `.pal` shows "Palette loaded."
      and a Replace row, and a too-short one shows "That palette file didn't
      work." with the reason in the problem log.
- [ ] From a game, the preview is the paused frame, "Done" returns to the
      pause menu, and the game takes the chosen look and palette at once.
- [ ] Settings → Play: with "Pause when the window loses focus" on, clicking
      another window opens the pause menu; turned off, the game plays on while
      another window has the keyboard, no key stays held, and the choice
      survives a restart.
- [ ] Controller buttons → Set up: four presses on the keyboard save it and
      say "Buttons saved for Keyboard…", and the new keys play; the same four
      on a controller save under its GUID in `controllers.json`; pressing one
      button twice is ignored; Space, Backspace, F5, F8 and F11 say "That key
      already does something. Pick another." and do not count as a step; Tab
      does count, and afterwards still plays the game rather than moving the
      panels' focus; Escape cancels.
- [ ] "Audio delay" in Settings shows about 30 ms while a game plays.

The separate external ROM accuracy suite requires the files described in
`core/tests/roms/README.md`; its absence is not an accuracy pass.
