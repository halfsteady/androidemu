# Emulia on macOS and Linux

The emulator was already portable: `nes-core` has no dependencies or platform
I/O, and `nes-runner` uses Rust's standard library. CPU, PPU, APU, cartridge
mappers, input registers, rewind and state serialization all live in `core/`.
The Android JNI and AAudio integration lives separately in `native/`.

`nes-desktop` supplies a small playable desktop frontend using that same core.
It provides a resizable, letterboxed 256×240 picture, 48 kHz mono sound,
one keyboard controller, pause/reset, one save-state slot per ROM and battery
save persistence. It does not yet include Android's library UI, picture settings,
gamepad mapping or rewind controls. Colour emphasis remains a core limitation.

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
sudo apt-get install build-essential cmake libx11-dev libxext-dev libxrandr-dev libxcursor-dev libxi-dev libxfixes-dev libxss-dev libasound2-dev libpulse-dev libwayland-dev libxkbcommon-dev libegl1-mesa-dev libgl1-mesa-dev
```

From the repository root:

```sh
cargo build --release --locked -p nes-desktop -p nes-runner
./target/release/nes-desktop "/path/to/game.nes"
```

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

## Play and saves

| Key | Action |
|---|---|
| Arrow keys | Direction pad |
| X / Z | A / B |
| Enter / Right Shift | Start / Select |
| Space | Pause/resume |
| R | Reset |
| F5 / F8 | Save/load state (use Fn on Macs where needed) |
| Escape | Quit |

Losing window focus pauses emulation and clears queued audio. Sound clocks the
emulation independently of display refresh rate; `--mute` uses the ROM region's
frame rate and a monotonic clock. Use `--mute` when no audio device is available.

Saves are keyed by ROM payload identity, so moving or renaming a ROM retains its
saves. The default directory is `~/Library/Application Support/Emulia` on macOS
and `$XDG_DATA_HOME/emulia` (or `~/.local/share/emulia`) on Linux. Override it with
`--save-dir /path/to/saves`, including a directory on removable storage.
Battery RAM is saved every five seconds during play and on clean exit. Save
replacement is atomic; malformed existing SRAM is reported and left intact.
Errors appear in the launching terminal. Avoid running two copies of the same
game against the same save directory, since the last writer wins.

F5/F8 use the existing core's versioned state format. Android save containers
and metadata are not imported automatically. Force-quitting can lose battery
changes since the last periodic save.

## Verification

```sh
cargo test --workspace --locked
python3 scripts/check-desktop.py target/release/nes-desktop
```

The smoke test creates its own NROM, runs the real frontend with SDL dummy video
and audio drivers, and checks SRAM persistence and invalid-input handling. CI
runs this on each OS/architecture above. Dummy drivers cannot verify physical
keyboard behavior, audible output or GPU presentation; check those interactively
with a ROM you own. The separate external ROM accuracy suite requires the files
described in `core/tests/roms/README.md`; its absence is not an accuracy pass.
