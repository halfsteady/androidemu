# Desktop Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `nes-desktop` the Android shell: a library shelf, pause menu, ten save slots with thumbnails, rewind/fast-forward scrubber, picture settings with the ten GLSL looks and four palettes, a gamepad mapping wizard and screenshots, on macOS and Linux.

**Architecture:** Keep the bundled, static SDL2 for the window, audio queue, focus and game controllers. Add an OpenGL 3.3 core context, draw the picture through the Android shaders ported to GLSL 330 with `glow`, and paint `egui` panels over it with `egui_glow` through our own SDL-to-egui event bridge. Everything except the window, the GL pipeline and the panels lives in modules that build and test without a display, and `--frames` runs those modules headless so the existing dummy-driver smoke test keeps working.

**Tech Stack:** Rust 2021, `sdl2 0.38` (bundled, static-link), `egui 0.36`, `egui_glow 0.36`, `glow 0.17`, `rfd 0.17`, `serde`/`serde_json 1`, `png 0.18`, `image 0.25` (png + jpeg only), `chrono 0.4`.

**Spec:** `docs/superpowers/specs/2026-09-10-desktop-shell-design.md`

## Global Constraints

- Every setting id, file name, string and confirmation wording comes from the spec and matches Android exactly (Look ids 0,1,2,3,4,6,7,8,9,10 with 5 retired to Old photo; palette ids 0,1,2,3,5 with 4 retired to Standard).
- Data directory default unchanged: `~/Library/Application Support/Emulia` on macOS, `$XDG_DATA_HOME/emulia` or `~/.local/share/emulia` on Linux. Library key is `format!("{:016x}", header.hash)`.
- All file writes go through the atomic temp-and-rename `write_atomic` (today's `write_save`).
- Frames are RGBA8, 256×240, top-down, everywhere on the desktop side (engine frame, thumbnails, sample frame, GL upload).
- `--frames <n>` must run with `SDL_VIDEODRIVER=dummy` and no GL context.
- `cargo clippy --locked -p nes-desktop --all-targets --no-deps -- -D warnings` stays clean; `cargo test --locked -p nes-desktop` passes after every task.
- No `Co-Authored-By` trailers or generated-with lines beyond what the session's attribution instructions require; never push.
- Commit after every task with the message shown; run `cargo fmt -p nes-desktop` before each commit.
- Run tests with `cargo test -p nes-desktop` from the repository root. The first build compiles SDL from source and takes a few minutes; later builds are incremental.

## File Structure

```
desktop/Cargo.toml              dependencies (Task 1)
desktop/src/main.rs             CLI, headless run, launch shell (Task 6, 9)
desktop/src/files.rs            write_atomic, read_optional, default_data_dir, pictures_dir, PNG read/write (Task 1)
desktop/src/scrub.rs            speed/label math (Task 1)
desktop/src/picture.rs          Aspect, Viewport, layout, Look, PaletteChoice, palette model, .pal, sample frame (Task 2)
desktop/src/settings.rs         Settings + Profiles JSON, migrations, palette resolution (Task 3)
desktop/src/library.rs          index, game folders, import, slots, thumbnails, art, archive, playtime, problem log (Task 4)
desktop/src/engine.rs           Nes + Rewind + palette + RGBA frame, save/load/reset, header notes (Task 5)
desktop/src/session.rs          Engine + Library paths + audio pacing + autosave + migration + screenshots (Task 5)
desktop/src/input.rs            keyboard/controller state, profiles, time control, wizard (Task 7)
desktop/src/video.rs            glow pipeline and preview readback (Task 8)
desktop/src/shaders/quad.vert, smooth.frag, composite.frag, present.frag (Task 8)
desktop/src/bridge.rs           SDL event -> egui RawInput, painter setup (Task 9)
desktop/src/shell.rs            window, GL, main loop, App state, action dispatch (Task 9, 11-14)
desktop/src/ui/mod.rs           App-facing types: Action enum, Panel enum, Dialog (Task 10)
desktop/src/ui/theme.rs         tokens and egui style (Task 10)
desktop/src/ui/widgets.rs       PrimaryAction, SecondaryAction, QuietAction, ActionTile, Panel frame, rows, chips (Task 10)
desktop/src/ui/shelf.rs         shelf screen (Task 11)
desktop/src/ui/time.rs          scrubber and skip buttons (Task 12)
desktop/src/ui/play.rs          title bar, chrome, HUD, busy (Task 12)
desktop/src/ui/panels.rs        pause, slots, dialogs, problem log (Task 13)
desktop/src/ui/settings.rs      settings panel and mapping wizard panel (Task 14)
scripts/check-desktop.py        headless smoke test, plus import check (Task 6)
scripts/check-shaders.py        also validates desktop GLSL (Task 8)
docs/DESKTOP.md, README.md      docs (Task 15)
```

---

### Task 1: Dependencies, shared file helpers and scrub math

**Files:**
- Modify: `desktop/Cargo.toml`
- Create: `desktop/src/files.rs`, `desktop/src/scrub.rs`
- Modify: `desktop/src/main.rs` (declare modules, move `write_save`/`read_optional`/`default_save_dir` into `files.rs`)

**Interfaces:**
- Produces: `files::write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String>`, `files::read_optional(path: &Path) -> Result<Option<Vec<u8>>, String>`, `files::default_data_dir() -> Result<PathBuf, String>`, `files::pictures_dir() -> Result<PathBuf, String>`, `files::write_png(path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String>`, `files::read_png(path) -> Result<(u32, u32, Vec<u8>), String>` (RGBA8), `files::now_millis() -> i64`
- Produces: `scrub::MAX: i32 = 8`, `scrub::DEAD_ZONE: f32 = 0.12`, `scrub::speed(fraction: f32) -> i32`, `scrub::label(speed: i32) -> String`

- [ ] **Step 1: Add dependencies**

Replace `desktop/Cargo.toml` `[dependencies]` with:

```toml
[dependencies]
nes-core = { path = "../core" }
sdl2 = { version = "0.38", features = ["bundled", "static-link"] }
egui = "0.36"
egui_glow = "0.36"
glow = "0.17"
rfd = "0.17"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
png = "0.18"
image = { version = "0.25", default-features = false, features = ["png", "jpeg"] }
chrono = "0.4"
```

Run: `cargo build -p nes-desktop` from the repository root. Expected: builds (the lockfile gains the new crates; commit `Cargo.lock`).

- [ ] **Step 2: Write the failing scrub tests**

Create `desktop/src/scrub.rs`:

```rust
//! One control runs time in both directions: drag left of centre to go back,
//! right to go forward, and the further from centre the faster it runs. Kept
//! pure because it is the part that has to feel right. Ported from Scrub.kt.

/// Frames per displayed frame at the far end of the track.
pub const MAX: i32 = 8;

/// The middle of the track is dead: a pointer resting slightly off centre
/// must not creep the game along.
pub const DEAD_ZONE: f32 = 0.12;

/// `fraction` runs -1 (fully left) to 1 (fully right). Negative results are
/// frames to step back per displayed frame, positive frames to run forward,
/// zero is ordinary play.
pub fn speed(fraction: f32) -> i32 {
    todo!()
}

/// What the heads-up display says while the track is held.
pub fn label(speed: i32) -> String {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_middle_of_the_track_is_stopped() {
        assert_eq!(speed(0.0), 0);
        assert_eq!(speed(DEAD_ZONE), 0);
        assert_eq!(speed(-DEAD_ZONE), 0);
        assert_eq!(speed(DEAD_ZONE + 0.001), 1);
        assert_eq!(speed(-DEAD_ZONE - 0.001), -1);
    }

    #[test]
    fn the_ends_of_the_track_are_the_fastest() {
        assert_eq!(speed(1.0), MAX);
        assert_eq!(speed(-1.0), -MAX);
        assert_eq!(speed(3.0), MAX);
        assert_eq!(speed(-3.0), -MAX);
    }

    #[test]
    fn speed_rises_with_distance_and_never_skips_backwards() {
        let mut previous = 0;
        for step in 0..=100 {
            let at = step as f32 / 100.0;
            let s = speed(at);
            assert!(s >= previous, "{at} went from {previous} to {s}");
            assert!((0..=MAX).contains(&s));
            assert_eq!(speed(-at), -s);
            previous = s;
        }
        assert_eq!(previous, MAX);
    }

    #[test]
    fn every_step_between_one_and_the_maximum_is_reachable() {
        let reached: std::collections::BTreeSet<i32> =
            (0..=100).map(|i| speed(i as f32 / 100.0)).collect();
        assert_eq!(reached, (0..=MAX).collect());
    }

    #[test]
    fn the_label_says_which_way_time_is_going() {
        assert_eq!(label(0), "Ready");
        assert_eq!(label(-3), "Rewinding 3×");
        assert_eq!(label(8), "Fast-forward 8×");
    }
}
```

Add `mod scrub;` and `mod files;` to `desktop/src/main.rs` (top, next to `mod palette;`).

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p nes-desktop scrub`
Expected: the five tests panic with `not yet implemented`.

- [ ] **Step 4: Implement scrub**

```rust
pub fn speed(fraction: f32) -> i32 {
    let magnitude = fraction.abs().min(1.0);
    if magnitude <= DEAD_ZONE {
        return 0;
    }
    let past = (magnitude - DEAD_ZONE) / (1.0 - DEAD_ZONE);
    let steps = ((past * MAX as f32).ceil() as i32).clamp(1, MAX);
    if fraction < 0.0 {
        -steps
    } else {
        steps
    }
}

pub fn label(speed: i32) -> String {
    match speed {
        s if s < 0 => format!("Rewinding {}×", -s),
        s if s > 0 => format!("Fast-forward {s}×"),
        _ => "Ready".to_string(),
    }
}
```

Run: `cargo test -p nes-desktop scrub` Expected: 5 passed.

- [ ] **Step 5: Create files.rs and move the helpers**

Create `desktop/src/files.rs`:

```rust
//! Files the shell writes, and where. Every write is atomic: a temp file in
//! the same directory, fsync, rename, so a crash never leaves a half-written
//! save, index or setting behind.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

/// Write in the same directory so rename atomically replaces the old file.
/// create_new avoids accidentally following an existing temp file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| format!("{}: {e}", temp.display()))?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(|e| format!("Cannot save {}: {e}", path.display()))
}

pub fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn home() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is unset; use --data-dir".to_string())
}

pub fn default_data_dir() -> Result<PathBuf, String> {
    if cfg!(target_os = "macos") {
        return Ok(home()?.join("Library/Application Support/Emulia"));
    }
    if let Some(path) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return Ok(path.join("emulia"));
    }
    Ok(home()?.join(".local/share/emulia"))
}

/// `~/Pictures/Emulia`, honouring `XDG_PICTURES_DIR` from user-dirs.dirs on Linux.
pub fn pictures_dir() -> Result<PathBuf, String> {
    let home = home()?;
    if !cfg!(target_os = "macos") {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        if let Ok(text) = fs::read_to_string(config.join("user-dirs.dirs")) {
            for line in text.lines() {
                if let Some(value) = line.trim().strip_prefix("XDG_PICTURES_DIR=") {
                    let value = value.trim_matches('"').replace("$HOME", &home.to_string_lossy());
                    return Ok(PathBuf::from(value).join("Emulia"));
                }
            }
        }
    }
    Ok(home.join("Pictures/Emulia"))
}

pub fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// RGBA8, top-down.
pub fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    }
    write_atomic(path, &out)
}

/// Any PNG the shell wrote, back as RGBA8 top-down.
pub fn read_png(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buffer = vec![0; reader.output_buffer_size().ok_or("PNG too large")?];
    let info = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
    buffer.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::Grayscale => buffer.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => {
            buffer.chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect()
        }
        other => return Err(format!("unsupported PNG colour type {other:?}")),
    };
    Ok((info.width, info.height, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulia-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_save_replaces_and_preserves_previous_file_on_collision() {
        let dir = temp_dir("files-atomic");
        let path = dir.join("game.sav");
        write_atomic(&path, b"old").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        let temp = path.with_extension(format!("tmp-{}", std::process::id()));
        fs::write(&temp, b"existing temporary file").unwrap();
        assert!(write_atomic(&path, b"replacement").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(fs::read(&temp).unwrap(), b"existing temporary file");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn png_round_trips_rgba() {
        let dir = temp_dir("files-png");
        let path = dir.join("thumb.png");
        let pixels: Vec<u8> = (0..4 * 3 * 2).map(|i| (i * 7) as u8).collect();
        write_png(&path, 3, 2, &pixels).unwrap();
        let (w, h, back) = read_png(&path).unwrap();
        assert_eq!((w, h), (3, 2));
        assert_eq!(back, pixels);
        fs::remove_dir_all(dir).unwrap();
    }
}
```

In `desktop/src/main.rs`: delete `write_save`, `read_optional`, `default_save_dir` and the `atomic_save_replaces_and_preserves_previous_file_on_collision` test; replace call sites with `files::write_atomic`, `files::read_optional`, `files::default_data_dir`. Keep everything else working for now.

- [ ] **Step 6: Run all desktop tests and clippy**

Run: `cargo test -p nes-desktop && cargo clippy -p nes-desktop --all-targets --no-deps -- -D warnings`
Expected: all pass, no warnings. (`pictures_dir` and `now_millis`, `read_png` may be unused: add `#[allow(dead_code)]` on the module line `mod files;` for now and remove it in Task 4.)

- [ ] **Step 7: Commit**

```bash
git add desktop/Cargo.toml Cargo.lock desktop/src/main.rs desktop/src/files.rs desktop/src/scrub.rs
git commit -m "Add desktop shell dependencies, atomic file helpers and scrub math"
```

---

### Task 2: Picture geometry, looks, palettes and the sample frame

**Files:**
- Create: `desktop/src/picture.rs`
- Modify: `desktop/src/main.rs` (add `mod picture;`)

**Interfaces:**
- Produces:
  - `picture::{WIDTH: usize = 256, HEIGHT: usize = 240, TRIM: i32 = 8}`
  - `enum Aspect { Television, Hardware, Pixels }` with `Aspect::ALL: [Aspect; 3]`, `label(self) -> &'static str`, `from_index(i: u32) -> Aspect`, `index(self) -> u32`
  - `struct Viewport { x: i32, y: i32, width: i32, height: i32 }` (`PartialEq, Debug, Clone, Copy`)
  - `visible_height(trim: bool) -> i32`, `trim_fraction(trim: bool) -> f32`, `layout(surface_width: i32, surface_height: i32, aspect: Aspect, trim: bool) -> Viewport`
  - `enum Source { Direct, Smoothed, Composite }`
  - `enum Look { Off, Scanlines, OldTv, DotMatrix, FourGreens, OldPhoto, Neon, Cartoon, Smooth, Composite }` with `Look::ALL: [Look; 10]`, `id(self) -> i32`, `label`, `note`, `source(self) -> Source`, `from_id(id: i32) -> Look`
  - `enum PaletteChoice { Standard, Hardware, Soft, Vivid, File }` with `ALL: [PaletteChoice; 5]`, `id`, `label`, `note`, `colours(self) -> Option<[u32; 64]>`, `from_id(id: i32) -> PaletteChoice`
  - `mod model { build(saturation: f64, tint: f64, contrast: f64, brightness: f64, gamma: f64) -> [u32; 64]; standard() -> [u32; 64]; bytes(&[u32; 64]) -> Vec<u8>; parse(&[u8]) -> Option<[u32; 64]> }`
  - `sample_frame(palette: &[u32; 64]) -> Vec<u8>` (RGBA, 256×240×4)

- [ ] **Step 1: Write the failing tests**

Create `desktop/src/picture.rs` with the type declarations above as stubs (`todo!()` bodies) and these tests at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn aspect_of(v: Viewport) -> f32 {
        v.width as f32 / v.height as f32
    }

    #[test]
    fn television_is_four_thirds_and_fits_the_surface() {
        let wide = layout(1000, 500, Aspect::Television, false);
        assert!((aspect_of(wide) - 4.0 / 3.0).abs() < 0.01);
        assert!(wide.width <= 1000 && wide.height <= 500);
        let tall = layout(600, 2000, Aspect::Television, false);
        assert!((aspect_of(tall) - 4.0 / 3.0).abs() < 0.01);
        assert_eq!(tall.width, 600);
    }

    #[test]
    fn hardware_is_narrower_than_television() {
        let television = layout(1000, 1000, Aspect::Television, false);
        let hardware = layout(1000, 1000, Aspect::Hardware, false);
        assert!((aspect_of(hardware) - 256.0 * 8.0 / 7.0 / 240.0).abs() < 0.01);
        assert!(hardware.height > television.height);
    }

    #[test]
    fn trimming_raises_the_aspect_and_narrows_the_source() {
        assert_eq!(visible_height(false), 240);
        assert_eq!(visible_height(true), 224);
        assert_eq!(trim_fraction(false), 0.0);
        assert!((trim_fraction(true) - 8.0 / 240.0).abs() < 0.0001);
        let whole = layout(1000, 1000, Aspect::Hardware, false);
        let trimmed = layout(1000, 1000, Aspect::Hardware, true);
        assert!(aspect_of(trimmed) > aspect_of(whole));
    }

    #[test]
    fn pixel_perfect_uses_whole_multiples_only() {
        assert_eq!(layout(1024, 960, Aspect::Pixels, false), Viewport { x: 0, y: 0, width: 1024, height: 960 });
        assert_eq!(layout(1000, 900, Aspect::Pixels, false), Viewport { x: 116, y: 90, width: 768, height: 720 });
        assert_eq!(layout(1024, 900, Aspect::Pixels, true), Viewport { x: 0, y: 2, width: 1024, height: 896 });
    }

    #[test]
    fn every_layout_is_centred_and_inside_the_surface() {
        for aspect in Aspect::ALL {
            for trim in [false, true] {
                for width in [1, 320, 721, 1000, 2400] {
                    for height in [1, 240, 519, 900, 1600] {
                        let v = layout(width, height, aspect, trim);
                        assert_eq!(v.x, (width - v.width) / 2);
                        assert_eq!(v.y, (height - v.height) / 2);
                        if aspect != Aspect::Pixels {
                            assert!(v.width <= width && v.height <= height);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_degenerate_surface_draws_nothing() {
        let none = Viewport { x: 0, y: 0, width: 0, height: 0 };
        assert_eq!(layout(0, 0, Aspect::Television, false), none);
        assert_eq!(layout(-4, 100, Aspect::Pixels, false), none);
    }

    #[test]
    fn look_ids_are_the_saved_values_so_they_must_not_move() {
        let ids: Vec<i32> = Look::ALL.iter().map(|l| l.id()).collect();
        assert_eq!(ids, vec![0, 1, 2, 3, 4, 6, 7, 8, 9, 10]);
        assert_eq!(Look::from_id(5), Look::OldPhoto);
        assert_eq!(Look::from_id(2), Look::OldTv);
        assert_eq!(Look::from_id(99), Look::Off);
        assert_eq!(Look::from_id(-1), Look::Off);
        let smoothed: Vec<Look> = Look::ALL.iter().copied().filter(|l| l.source() == Source::Smoothed).collect();
        assert_eq!(smoothed, vec![Look::Cartoon, Look::Smooth]);
        let composite: Vec<Look> = Look::ALL.iter().copied().filter(|l| l.source() == Source::Composite).collect();
        assert_eq!(composite, vec![Look::Composite]);
        for look in Look::ALL {
            assert!(!look.label().is_empty() && !look.note().is_empty());
        }
    }

    #[test]
    fn palette_ids_are_the_saved_values_so_they_must_not_move() {
        let ids: Vec<i32> = PaletteChoice::ALL.iter().map(|p| p.id()).collect();
        assert_eq!(ids, vec![0, 1, 2, 3, 5]);
        assert_eq!(PaletteChoice::from_id(4), PaletteChoice::Standard);
        assert_eq!(PaletteChoice::from_id(99), PaletteChoice::Standard);
        assert_eq!(PaletteChoice::from_id(5), PaletteChoice::File);
        assert!(PaletteChoice::Standard.colours().is_none());
        assert!(PaletteChoice::File.colours().is_none());
        for p in [PaletteChoice::Hardware, PaletteChoice::Soft, PaletteChoice::Vivid] {
            for c in p.colours().unwrap() {
                assert_eq!(c & 0xff00_0000, 0);
            }
        }
    }

    fn channels(c: u32) -> [i32; 3] {
        [((c >> 16) & 255) as i32, ((c >> 8) & 255) as i32, (c & 255) as i32]
    }

    #[test]
    fn the_model_reproduces_the_table_the_core_ships() {
        let model = model::standard();
        let mut errors = Vec::new();
        for index in 0..64 {
            if index & 15 >= 14 {
                continue;
            }
            let core = crate::palette::PALETTE[index];
            if core == 0 && (index & 15) != 13 {
                continue;
            }
            let (a, b) = (channels(model[index]), channels(core));
            errors.push((0..3).map(|i| (a[i] - b[i]).abs()).max().unwrap());
        }
        assert!(errors.len() >= 50);
        let mean = errors.iter().sum::<i32>() as f64 / errors.len() as f64;
        assert!(mean < 15.0, "mean error {mean}");
        assert!(*errors.iter().max().unwrap() < 40);
    }

    #[test]
    fn the_blanking_colours_are_never_brighter_than_the_darkest_real_one() {
        fn luma(c: u32) -> f64 {
            let [r, g, b] = channels(c);
            0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64
        }
        for knobs in [model::build(1.0, 0.0, 1.0, 0.0, 1.0), model::build(2.0, 0.0, 1.5, 0.2, 1.0), model::standard()] {
            let blanking: Vec<f64> = (0..64).filter(|i| i & 15 >= 14).map(|i| luma(knobs[i])).collect();
            let real: Vec<f64> = (0..64).filter(|i| i & 15 < 13).map(|i| luma(knobs[i])).collect();
            assert!(blanking.iter().all(|&l| l == blanking[0]));
            let max_blank = blanking.iter().cloned().fold(f64::MIN, f64::max);
            let min_real = real.iter().cloned().fold(f64::MAX, f64::min);
            assert!(max_blank <= min_real + 0.001);
        }
        let plain = model::build(1.0, 0.0, 1.0, 0.0, 1.0);
        for level in 0..4 {
            for hue in 14..16 {
                assert_eq!(plain[level << 4 | hue], 0);
            }
        }
    }

    #[test]
    fn turning_the_colour_down_leaves_greys() {
        for c in model::build(0.0, 0.0, 1.0, 0.0, 1.0) {
            let ch = channels(c);
            assert!(ch.iter().max().unwrap() - ch.iter().min().unwrap() <= 2);
        }
    }

    #[test]
    fn a_palette_survives_the_round_trip_to_bytes_and_back() {
        let original = model::build(1.2, 0.5, 1.0, 0.0, 1.0);
        assert_eq!(model::parse(&model::bytes(&original)), Some(original));
    }

    #[test]
    fn a_file_too_short_to_be_a_palette_is_refused() {
        assert!(model::parse(&[0; 191]).is_none());
        assert!(model::parse(&[0; 192]).is_some());
        assert!(model::parse(&[0; 64 * 3 * 8]).is_some());
    }

    #[test]
    fn the_sample_frame_is_a_whole_opaque_framebuffer_with_something_for_every_look() {
        let pixels = sample_frame(&model::standard());
        assert_eq!(pixels.len(), WIDTH * HEIGHT * 4);
        assert!(pixels.iter().skip(3).step_by(4).all(|&a| a == 255));
        let luma = |x: usize, y: usize| {
            let at = (y * WIDTH + x) * 4;
            pixels[at] as i32 + pixels[at + 1] as i32 + pixels[at + 2] as i32
        };
        let bars: std::collections::BTreeSet<i32> = (0..WIDTH).step_by(8).map(|x| luma(x, 20)).collect();
        assert!(bars.len() > 4);
        assert!(luma(240, 100) > luma(8, 100));
        assert_ne!(luma(10, 140), luma(11, 140));
        let band: Vec<i32> = (168..HEIGHT).step_by(4).flat_map(|y| (0..WIDTH).step_by(4).map(move |x| (x, y))).map(|(x, y)| luma(x, y)).collect();
        assert!(band.iter().max().unwrap() > &(band.iter().min().unwrap() + 300));
    }
}
```

Run: `cargo test -p nes-desktop picture` Expected: compile succeeds and tests panic on `todo!()`.

- [ ] **Step 2: Implement picture.rs**

```rust
//! The presentation geometry, the looks, and what the framebuffer's 64
//! indices mean in colour. Ported from Picture.kt, Filter.kt, Palette.kt and
//! SampleFrame.kt; the unit tests mirror the Kotlin ones.

pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 240;
/// Rows hidden at the top and bottom when edges are trimmed.
pub const TRIM: i32 = 8;
/// The 2C02 emitted pixels 8/7 as wide as they were tall.
const PIXEL_ASPECT: f32 = 8.0 / 7.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aspect {
    Television,
    Hardware,
    Pixels,
}

impl Aspect {
    pub const ALL: [Aspect; 3] = [Aspect::Television, Aspect::Hardware, Aspect::Pixels];
    pub fn label(self) -> &'static str {
        match self {
            Aspect::Television => "4:3 television",
            Aspect::Hardware => "8:7 hardware",
            Aspect::Pixels => "Pixel-perfect",
        }
    }
    pub fn index(self) -> u32 {
        self as u32
    }
    pub fn from_index(i: u32) -> Aspect {
        Aspect::ALL.get(i as usize).copied().unwrap_or(Aspect::Television)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewport {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub fn visible_height(trim: bool) -> i32 {
    if trim { HEIGHT as i32 - 2 * TRIM } else { HEIGHT as i32 }
}

pub fn trim_fraction(trim: bool) -> f32 {
    if trim { TRIM as f32 / HEIGHT as f32 } else { 0.0 }
}

/// The largest presentation of the visible area that fits the surface, centred.
/// Television and hardware scale continuously; pixel-perfect drops to the next
/// whole multiple, so it leaves a wider border rather than resample.
pub fn layout(surface_width: i32, surface_height: i32, aspect: Aspect, trim: bool) -> Viewport {
    let visible = visible_height(trim);
    if surface_width <= 0 || surface_height <= 0 {
        return Viewport { x: 0, y: 0, width: 0, height: 0 };
    }
    let (width, height) = match aspect {
        Aspect::Pixels => {
            let scale = (surface_width / WIDTH as i32).min(surface_height / visible).max(1);
            (WIDTH as i32 * scale, visible * scale)
        }
        _ => {
            let target = if aspect == Aspect::Television {
                4.0 / 3.0
            } else {
                WIDTH as f32 * PIXEL_ASPECT / visible as f32
            };
            let width = (surface_width as f32).min(surface_height as f32 * target);
            (width as i32, (width / target) as i32)
        }
    };
    Viewport { x: (surface_width - width) / 2, y: (surface_height - height) / 2, width, height }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Direct,
    Smoothed,
    Composite,
}

/// A look applied to the finished framebuffer. `id` is what gets saved, not
/// the position; a retired id is never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Off,
    Scanlines,
    OldTv,
    DotMatrix,
    FourGreens,
    OldPhoto,
    Neon,
    Cartoon,
    Smooth,
    Composite,
}

impl Look {
    pub const ALL: [Look; 10] = [
        Look::Off, Look::Scanlines, Look::OldTv, Look::DotMatrix, Look::FourGreens,
        Look::OldPhoto, Look::Neon, Look::Cartoon, Look::Smooth, Look::Composite,
    ];
    /// 5 was "Black and white": Old photo with the tint thrown away. Retired.
    const RETIRED_GREY: i32 = 5;

    pub fn id(self) -> i32 {
        match self {
            Look::Off => 0, Look::Scanlines => 1, Look::OldTv => 2, Look::DotMatrix => 3,
            Look::FourGreens => 4, Look::OldPhoto => 6, Look::Neon => 7, Look::Cartoon => 8,
            Look::Smooth => 9, Look::Composite => 10,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Look::Off => "Off", Look::Scanlines => "Scanlines", Look::OldTv => "Old TV",
            Look::DotMatrix => "Dot matrix", Look::FourGreens => "Four greens",
            Look::OldPhoto => "Old photo", Look::Neon => "Neon", Look::Cartoon => "Cartoon",
            Look::Smooth => "Smooth", Look::Composite => "Composite",
        }
    }
    pub fn note(self) -> &'static str {
        match self {
            Look::Off => "The framebuffer exactly as the console drew it",
            Look::Scanlines => "A soft dark band between each row, the way a CRT drew them",
            Look::OldTv => "Scanlines, a curved tube, an aperture grille and a darkened edge",
            Look::DotMatrix => "A grid between the pixels, like a handheld's LCD",
            Look::FourGreens => "Everything remapped onto a handheld's four-shade screen",
            Look::OldPhoto => "Warm and faded",
            Look::Neon => "Bright things glow and the colour is turned all the way up",
            Look::Cartoon => "Smoothed into curves, inked, and painted in flat colour",
            Look::Smooth => "Stairsteps rounded into curves, with the colour left alone",
            Look::Composite => "Down an aerial lead: colours bleed and dithering turns solid",
        }
    }
    pub fn source(self) -> Source {
        match self {
            Look::Cartoon | Look::Smooth => Source::Smoothed,
            Look::Composite => Source::Composite,
            _ => Source::Direct,
        }
    }
    pub fn from_id(id: i32) -> Look {
        if id == Self::RETIRED_GREY {
            return Look::OldPhoto;
        }
        Look::ALL.into_iter().find(|l| l.id() == id).unwrap_or(Look::Off)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteChoice {
    Standard,
    Hardware,
    Soft,
    Vivid,
    File,
}

impl PaletteChoice {
    pub const ALL: [PaletteChoice; 5] = [
        PaletteChoice::Standard, PaletteChoice::Hardware, PaletteChoice::Soft,
        PaletteChoice::Vivid, PaletteChoice::File,
    ];
    pub fn id(self) -> i32 {
        match self {
            PaletteChoice::Standard => 0, PaletteChoice::Hardware => 1,
            PaletteChoice::Soft => 2, PaletteChoice::Vivid => 3, PaletteChoice::File => 5,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            PaletteChoice::Standard => "Standard", PaletteChoice::Hardware => "Hardware",
            PaletteChoice::Soft => "Soft", PaletteChoice::Vivid => "Vivid",
            PaletteChoice::File => "From a file",
        }
    }
    pub fn note(self) -> &'static str {
        match self {
            PaletteChoice::Standard => "The colours the app has always used",
            PaletteChoice::Hardware => "Decoded from what the chip actually emitted, nothing added",
            PaletteChoice::Soft => "Gentler colour, the way a television left at the factory setting looked",
            PaletteChoice::Vivid => "Colour turned up, the way most people ran theirs",
            PaletteChoice::File => "A .pal palette you imported",
        }
    }
    /// The table, or None for Standard (the core's own) and File (supplied at run time).
    pub fn colours(self) -> Option<[u32; 64]> {
        match self {
            PaletteChoice::Standard | PaletteChoice::File => None,
            PaletteChoice::Hardware => Some(model::build(1.0, 0.0, 1.0, 0.0, 1.0)),
            PaletteChoice::Soft => Some(model::build(0.78, 0.0, 0.96, 0.02, 1.06)),
            PaletteChoice::Vivid => Some(model::build(1.30, 0.0, 1.05, 0.0, 0.94)),
        }
    }
    pub fn from_id(id: i32) -> PaletteChoice {
        PaletteChoice::ALL.into_iter().find(|p| p.id() == id).unwrap_or(PaletteChoice::Standard)
    }
}

/// The 2C02 emitted a square wave, not RGB. These colours are generated by
/// doing what the chip did and decoding it the way a television did, so the
/// knobs are the ones a television had.
pub mod model {
    use std::f64::consts::PI;

    const LOW: [f64; 4] = [0.350, 0.518, 0.962, 1.550];
    const HIGH: [f64; 4] = [1.094, 1.506, 1.962, 1.962];
    const BLACK: f64 = LOW[1];
    const WHITE: f64 = HIGH[3];
    const BURST: f64 = 4.0;
    const SAMPLES: usize = 12;

    fn signal(hue: usize, level: usize, sample: usize) -> f64 {
        match hue {
            0 => HIGH[level],
            13 => LOW[level],
            h if h >= 14 => LOW[1],
            _ => if (sample + hue) % SAMPLES < SAMPLES / 2 { HIGH[level] } else { LOW[level] },
        }
    }

    fn channel(v: f64, gamma: f64) -> u32 {
        (255.0 * v.clamp(0.0, 1.0).powf(gamma)).round().clamp(0.0, 255.0) as u32
    }

    pub fn build(saturation: f64, tint: f64, contrast: f64, brightness: f64, gamma: f64) -> [u32; 64] {
        let mut out = [0; 64];
        for (index, slot) in out.iter_mut().enumerate() {
            let hue = index & 15;
            let level = (index >> 4) & 3;
            let (mut y, mut i, mut q) = (0.0, 0.0, 0.0);
            for sample in 0..SAMPLES {
                let volts = (signal(hue, level, sample) - BLACK) / (WHITE - BLACK);
                let angle = 2.0 * PI * (sample as f64 + BURST + tint) / SAMPLES as f64;
                y += volts;
                i += volts * angle.cos();
                q += volts * angle.sin();
            }
            y = y / SAMPLES as f64 * contrast + brightness;
            i = i / SAMPLES as f64 * saturation * 2.0;
            q = q / SAMPLES as f64 * saturation * 2.0;
            *slot = channel(y + 0.956 * i + 0.619 * q, gamma) << 16
                | channel(y - 0.272 * i - 0.647 * q, gamma) << 8
                | channel(y - 1.106 * i + 1.703 * q, gamma);
        }
        out
    }

    /// The model's closest match to the table the core ships.
    pub fn standard() -> [u32; 64] {
        build(0.90, 0.0, 1.0, 0.0, 1.0)
    }

    pub fn bytes(colours: &[u32; 64]) -> Vec<u8> {
        colours.iter().flat_map(|c| [(c >> 16) as u8, (c >> 8) as u8, *c as u8]).collect()
    }

    /// A `.pal` file: 64 colours of RGB. Longer files carry emphasis variants, ignored.
    pub fn parse(file: &[u8]) -> Option<[u32; 64]> {
        if file.len() < 64 * 3 {
            return None;
        }
        let mut out = [0; 64];
        for (at, slot) in out.iter_mut().enumerate() {
            *slot = (file[at * 3] as u32) << 16 | (file[at * 3 + 1] as u32) << 8 | file[at * 3 + 2] as u32;
        }
        Some(out)
    }
}

/// A framebuffer to preview looks against when no game is open: colour bars
/// from palette indices (so a palette change restains them), a ramp, a
/// one-pixel checkerboard and bright blocks on a dark field. RGBA, top-down.
pub fn sample_frame(palette: &[u32; 64]) -> Vec<u8> {
    const BAR_INDICES: [usize; 8] = [0x30, 0x28, 0x2a, 0x1a, 0x2c, 0x21, 0x16, 0x14];
    let bars: Vec<[u8; 3]> = BAR_INDICES
        .iter()
        .map(|&i| {
            let c = palette[i];
            [(c >> 16) as u8, (c >> 8) as u8, c as u8]
        })
        .collect();
    let mut out = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let rgb = if y < 72 {
                bars[(x * bars.len() / WIDTH).min(bars.len() - 1)]
            } else if y < 120 {
                [x as u8, x as u8, x as u8]
            } else if y < 168 {
                if (x + y) % 2 == 0 { [232, 232, 240] } else { [24, 24, 40] }
            } else if (16..48).contains(&(x % 64)) && (8..32).contains(&(y % 40)) {
                [248, 216, 96]
            } else {
                [16, 20, 32]
            };
            let at = (y * WIDTH + x) * 4;
            out[at..at + 3].copy_from_slice(&rgb);
            out[at + 3] = 255;
        }
    }
    out
}
```

Add `mod picture;` to `main.rs` (with `#[allow(dead_code)]` until Task 9 uses everything).

- [ ] **Step 3: Run the tests and clippy**

Run: `cargo test -p nes-desktop picture && cargo clippy -p nes-desktop --all-targets --no-deps -- -D warnings`
Expected: 14 tests pass, no warnings.

- [ ] **Step 4: Commit**

```bash
git add desktop/src/picture.rs desktop/src/main.rs
git commit -m "Port picture geometry, looks, palette model and sample frame to desktop"
```

---

### Task 3: Settings and controller profiles on disk

**Files:**
- Create: `desktop/src/settings.rs`
- Modify: `desktop/src/main.rs` (add `mod settings;`)

**Interfaces:**
- Consumes: `picture::{Aspect, Look, PaletteChoice, model}`, `files::{write_atomic, read_optional}`, `palette::PALETTE`
- Produces:
  - `struct Settings { aspect: Aspect, look: Look, palette: PaletteChoice, trim_edges: bool, shelf_list: bool, fullscreen: bool }` (`Clone, Debug, PartialEq`), `Settings::default()`, `Settings::load(dir: &Path) -> Settings`, `Settings::save(&self, dir: &Path) -> Result<(), String>`
  - `imported_palette(dir: &Path) -> Option<[u32; 64]>`, `store_palette(dir: &Path, bytes: &[u8]) -> Result<[u32; 64], String>` (validates with `model::parse`)
  - `Settings::colours(&self, dir: &Path) -> [u32; 64]` (Standard is `palette::PALETTE`; File falls back to `PALETTE` when missing), `Settings::preview_colours(&self, dir) -> [u32; 64]` (Standard becomes `model::standard()` for the sample frame)
  - `struct Profile { name: String, a: String, b: String, select: String, start: String }` (`Clone, Debug, PartialEq, Serialize, Deserialize`)
  - `struct Profiles(pub BTreeMap<String, Profile>)` with `load(dir) -> Profiles`, `save(&self, dir) -> Result<(), String>`, `get(&self, key: &str) -> Option<&Profile>`, `set(&mut self, key: String, profile: Profile)`
  - `Profiles::KEYBOARD: &str = "keyboard"`

- [ ] **Step 1: Write the failing tests**

Create `desktop/src/settings.rs` with the interface as stubs and these tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("emulia-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn defaults_when_nothing_is_saved() {
        let dir = temp_dir("settings-default");
        let s = Settings::load(&dir);
        assert_eq!(s, Settings::default());
        assert_eq!(s.aspect, Aspect::Television);
        assert_eq!(s.look, Look::Off);
        assert_eq!(s.palette, PaletteChoice::Standard);
        assert!(!s.trim_edges && !s.shelf_list && !s.fullscreen);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn settings_round_trip_by_id_not_position() {
        let dir = temp_dir("settings-roundtrip");
        let s = Settings { aspect: Aspect::Pixels, look: Look::Composite, palette: PaletteChoice::Vivid, trim_edges: true, shelf_list: true, fullscreen: true };
        s.save(&dir).unwrap();
        let text = fs::read_to_string(dir.join("settings.json")).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["look"], 10);
        assert_eq!(json["palette"], 3);
        assert_eq!(json["aspect"], 2);
        assert_eq!(Settings::load(&dir), s);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn retired_ids_and_unknown_keys_are_tolerated() {
        let dir = temp_dir("settings-retired");
        fs::write(dir.join("settings.json"), r#"{"look": 5, "palette": 4, "aspect": 7, "future": true}"#).unwrap();
        let s = Settings::load(&dir);
        assert_eq!(s.look, Look::OldPhoto);
        assert_eq!(s.palette, PaletteChoice::Standard);
        assert_eq!(s.aspect, Aspect::Television);
        fs::write(dir.join("settings.json"), "not json").unwrap();
        assert_eq!(Settings::load(&dir), Settings::default());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_missing_imported_palette_falls_back_to_standard() {
        let dir = temp_dir("settings-palette");
        let s = Settings { palette: PaletteChoice::File, ..Settings::default() };
        assert_eq!(s.colours(&dir), crate::palette::PALETTE);
        assert!(imported_palette(&dir).is_none());
        assert!(store_palette(&dir, &[0; 100]).is_err());
        let table = model::build(1.1, 0.0, 1.0, 0.0, 1.0);
        assert_eq!(store_palette(&dir, &model::bytes(&table)).unwrap(), table);
        assert_eq!(imported_palette(&dir), Some(table));
        assert_eq!(s.colours(&dir), table);
        assert_eq!(Settings::default().colours(&dir), crate::palette::PALETTE);
        assert_eq!(Settings::default().preview_colours(&dir), model::standard());
        assert_eq!(Settings { palette: PaletteChoice::Soft, ..Settings::default() }.colours(&dir), PaletteChoice::Soft.colours().unwrap());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn controller_profiles_round_trip() {
        let dir = temp_dir("settings-profiles");
        let mut p = Profiles::load(&dir);
        assert!(p.get(Profiles::KEYBOARD).is_none());
        p.set("030000".into(), Profile { name: "Pad".into(), a: "b".into(), b: "a".into(), select: "back".into(), start: "start".into() });
        p.save(&dir).unwrap();
        let back = Profiles::load(&dir);
        assert_eq!(back.get("030000").unwrap().name, "Pad");
        assert_eq!(back.get("030000").unwrap().a, "b");
        fs::remove_dir_all(dir).unwrap();
    }
}
```

Run: `cargo test -p nes-desktop settings` Expected: panics on `todo!()`.

- [ ] **Step 2: Implement settings.rs**

```rust
//! The handful of choices that outlive a session, as one small JSON file,
//! and the controller button profiles as another. Ids on disk match the
//! Android app so retired ids map the same way.

use crate::files::{read_optional, write_atomic};
use crate::picture::{model, Aspect, Look, PaletteChoice};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub aspect: Aspect,
    pub look: Look,
    pub palette: PaletteChoice,
    pub trim_edges: bool,
    pub shelf_list: bool,
    pub fullscreen: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct OnDisk {
    #[serde(default)]
    aspect: u32,
    #[serde(default)]
    look: i32,
    #[serde(default)]
    palette: i32,
    #[serde(default, rename = "trimEdges")]
    trim_edges: bool,
    #[serde(default, rename = "shelfList")]
    shelf_list: bool,
    #[serde(default)]
    fullscreen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            aspect: Aspect::Television,
            look: Look::Off,
            palette: PaletteChoice::Standard,
            trim_edges: false,
            shelf_list: false,
            fullscreen: false,
        }
    }
}

const SETTINGS: &str = "settings.json";
const PALETTE_FILE: &str = "palette.pal";
const CONTROLLERS: &str = "controllers.json";

impl Settings {
    pub fn load(dir: &Path) -> Settings {
        let Ok(Some(bytes)) = read_optional(&dir.join(SETTINGS)) else {
            return Settings::default();
        };
        let Ok(disk) = serde_json::from_slice::<OnDisk>(&bytes) else {
            return Settings::default();
        };
        Settings {
            aspect: Aspect::from_index(disk.aspect),
            look: Look::from_id(disk.look),
            palette: PaletteChoice::from_id(disk.palette),
            trim_edges: disk.trim_edges,
            shelf_list: disk.shelf_list,
            fullscreen: disk.fullscreen,
        }
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let disk = OnDisk {
            aspect: self.aspect.index(),
            look: self.look.id(),
            palette: self.palette.id(),
            trim_edges: self.trim_edges,
            shelf_list: self.shelf_list,
            fullscreen: self.fullscreen,
        };
        let text = serde_json::to_string_pretty(&disk).map_err(|e| e.to_string())?;
        write_atomic(&dir.join(SETTINGS), text.as_bytes())
    }

    /// The colours the picture is painted with. Standard is the core's own
    /// table; an imported palette that has gone missing falls back to it.
    pub fn colours(&self, dir: &Path) -> [u32; 64] {
        match self.palette {
            PaletteChoice::Standard => crate::palette::PALETTE,
            PaletteChoice::File => imported_palette(dir).unwrap_or(crate::palette::PALETTE),
            other => other.colours().unwrap_or(crate::palette::PALETTE),
        }
    }

    /// The same, with the model's closest match standing in for Standard so
    /// the built sample frame has real colours to draw with.
    pub fn preview_colours(&self, dir: &Path) -> [u32; 64] {
        match self.palette {
            PaletteChoice::Standard => model::standard(),
            _ => self.colours(dir),
        }
    }
}

pub fn imported_palette(dir: &Path) -> Option<[u32; 64]> {
    read_optional(&dir.join(PALETTE_FILE)).ok().flatten().and_then(|b| model::parse(&b))
}

pub fn store_palette(dir: &Path, bytes: &[u8]) -> Result<[u32; 64], String> {
    let table = model::parse(bytes).ok_or("That palette file didn't work.")?;
    write_atomic(&dir.join(PALETTE_FILE), bytes)?;
    Ok(table)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub a: String,
    pub b: String,
    pub select: String,
    pub start: String,
}

#[derive(Default)]
pub struct Profiles(pub BTreeMap<String, Profile>);

impl Profiles {
    pub const KEYBOARD: &'static str = "keyboard";

    pub fn load(dir: &Path) -> Profiles {
        let map = read_optional(&dir.join(CONTROLLERS))
            .ok()
            .flatten()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Profiles(map)
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(&self.0).map_err(|e| e.to_string())?;
        write_atomic(&dir.join(CONTROLLERS), text.as_bytes())
    }

    pub fn get(&self, key: &str) -> Option<&Profile> {
        self.0.get(key)
    }

    pub fn set(&mut self, key: String, profile: Profile) {
        self.0.insert(key, profile);
    }
}
```

- [ ] **Step 3: Run tests and clippy**

Run: `cargo test -p nes-desktop settings && cargo clippy -p nes-desktop --all-targets --no-deps -- -D warnings`
Expected: 5 pass, clean.

- [ ] **Step 4: Commit**

```bash
git add desktop/src/settings.rs desktop/src/main.rs
git commit -m "Persist desktop picture settings and controller profiles as JSON"
```

---

### Task 4: The library on disk

**Files:**
- Create: `desktop/src/library.rs`
- Modify: `desktop/src/main.rs` (add `mod library;`)

**Interfaces:**
- Consumes: `files::{write_atomic, read_optional, write_png, read_png, now_millis}`
- Produces:
  - `struct Game { id: String, title: String, added: i64, played: i64, seconds: i64, archived: bool }` (`Clone, Debug, PartialEq, Serialize, Deserialize`)
  - `enum Slot { Auto, Number(u8) }` (`Clone, Copy, PartialEq, Debug`)
  - `struct SaveSlot { number: u8, time: Option<i64>, thumbnail: PathBuf }`
  - `struct Library { root: PathBuf }`:
    `open(data_dir: &Path) -> Result<Library, String>` (creates `<data>/library`),
    `games(&self) -> Vec<Game>`, `archived(&self) -> Vec<Game>`, `find(&self, id: &str) -> Option<Game>`,
    `add(&self, id: &str, title: &str, rom: &[u8]) -> Result<Game, String>`,
    `set_archived(&self, id: &str, archived: bool) -> Result<(), String>`, `forget(&self, id: &str) -> Result<(), String>`,
    `record(&self, id: &str, seconds: i64) -> Result<(), String>`,
    `directory(&self, id: &str) -> PathBuf`, `rom_path(&self, id) -> PathBuf`, `battery_path(&self, id) -> PathBuf`,
    `state_path(&self, id, slot: Slot) -> PathBuf`, `thumbnail_path(&self, id, slot: Slot) -> PathBuf`, `art_path(&self, id) -> PathBuf`,
    `slots(&self, id) -> Vec<SaveSlot>` (ten), `has_autosave(&self, id) -> bool`, `cover(&self, id) -> Option<PathBuf>`,
    `set_art(&self, id, image_path: &Path) -> Result<(), String>`, `clear_art(&self, id) -> Result<(), String>`,
    `log_problem(&self, label: &str, detail: &str)`, `problems(&self) -> Vec<String>` (newest first)
  - `read_import(path: &Path) -> Result<(String, Vec<u8>), String>` (title = file stem, 16 MB bound)
  - `playtime(seconds: i64) -> Option<String>`
  - `const ART_MAX_EDGE: u32 = 1024`, `const MAX_ROM: usize = 16 * 1024 * 1024`, `const PROBLEM_LINES: usize = 200`

- [ ] **Step 1: Write the failing tests**

Create `desktop/src/library.rs` with stubs and these tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulia-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn adding_dedupes_and_orders_by_last_played_then_added() {
        let dir = temp_dir("library-add");
        let lib = Library::open(&dir).unwrap();
        assert!(lib.games().is_empty());
        let a = lib.add("aaaa", "Game A", b"rom a").unwrap();
        let b = lib.add("bbbb", "Game B", b"rom b").unwrap();
        assert_eq!(lib.add("aaaa", "Renamed", b"other").unwrap(), a);
        assert_eq!(fs::read(lib.rom_path("aaaa")).unwrap(), b"rom a");
        assert!(b.added >= a.added);
        lib.record("aaaa", 90).unwrap();
        let games = lib.games();
        assert_eq!(games[0].id, "aaaa");
        assert_eq!(games[0].seconds, 90);
        assert!(games[0].played > 0);
        lib.record("aaaa", -5).unwrap();
        assert_eq!(lib.find("aaaa").unwrap().seconds, 90);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn archiving_is_reversible_and_forgetting_removes_the_folder() {
        let dir = temp_dir("library-archive");
        let lib = Library::open(&dir).unwrap();
        lib.add("aaaa", "Game A", b"rom").unwrap();
        lib.set_archived("aaaa", true).unwrap();
        assert!(lib.games().is_empty());
        assert_eq!(lib.archived().len(), 1);
        lib.set_archived("aaaa", false).unwrap();
        assert_eq!(lib.games().len(), 1);
        lib.forget("aaaa").unwrap();
        assert!(lib.games().is_empty() && lib.archived().is_empty());
        assert!(!lib.directory("aaaa").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_older_index_without_new_fields_still_loads() {
        let dir = temp_dir("library-old");
        fs::create_dir_all(dir.join("library")).unwrap();
        fs::write(dir.join("library/index.json"), r#"[{"id":"abcd","title":"Old","added":5}]"#).unwrap();
        let lib = Library::open(&dir).unwrap();
        let g = &lib.games()[0];
        assert_eq!((g.played, g.seconds, g.archived), (0, 0, false));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn slots_thumbnails_and_covers_follow_the_android_layout() {
        let dir = temp_dir("library-slots");
        let lib = Library::open(&dir).unwrap();
        lib.add("aaaa", "Game A", b"rom").unwrap();
        assert_eq!(lib.state_path("aaaa", Slot::Auto), lib.directory("aaaa").join("auto.state"));
        assert_eq!(lib.state_path("aaaa", Slot::Number(3)), lib.directory("aaaa").join("slot-3.state"));
        assert_eq!(lib.thumbnail_path("aaaa", Slot::Number(0)), lib.directory("aaaa").join("slot-0.png"));
        assert_eq!(lib.battery_path("aaaa"), lib.directory("aaaa").join("battery.sav"));
        let slots = lib.slots("aaaa");
        assert_eq!(slots.len(), 10);
        assert!(slots.iter().all(|s| s.time.is_none()));
        assert!(lib.cover("aaaa").is_none());
        assert!(!lib.has_autosave("aaaa"));
        fs::write(lib.state_path("aaaa", Slot::Number(2)), b"state").unwrap();
        assert!(lib.slots("aaaa")[2].time.is_some());
        crate::files::write_png(&lib.thumbnail_path("aaaa", Slot::Auto), 1, 1, &[1, 2, 3, 255]).unwrap();
        fs::write(lib.state_path("aaaa", Slot::Auto), b"state").unwrap();
        assert!(lib.has_autosave("aaaa"));
        assert_eq!(lib.cover("aaaa"), Some(lib.thumbnail_path("aaaa", Slot::Auto)));
        let big: Vec<u8> = vec![200; 2048 * 512 * 4];
        let source = dir.join("art.png");
        crate::files::write_png(&source, 2048, 512, &big).unwrap();
        lib.set_art("aaaa", &source).unwrap();
        assert_eq!(lib.cover("aaaa"), Some(lib.art_path("aaaa")));
        let (w, h, _) = crate::files::read_png(&lib.art_path("aaaa")).unwrap();
        assert_eq!((w, h), (1024, 256));
        lib.clear_art("aaaa").unwrap();
        assert_eq!(lib.cover("aaaa"), Some(lib.thumbnail_path("aaaa", Slot::Auto)));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_problem_log_is_newest_first_and_bounded() {
        let dir = temp_dir("library-problems");
        let lib = Library::open(&dir).unwrap();
        assert!(lib.problems().is_empty());
        for i in 0..250 {
            lib.log_problem("Something", &format!("detail {i}"));
        }
        let lines = lib.problems();
        assert_eq!(lines.len(), PROBLEM_LINES);
        assert!(lines[0].ends_with("Something — detail 249"));
        assert!(lines[0].len() > "yyyy-MM-dd HH:mm:ss  ".len());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn imports_take_the_file_stem_and_refuse_huge_files() {
        let dir = temp_dir("library-import");
        let path = dir.join("Super Game (USA).nes");
        fs::write(&path, b"NES\x1a").unwrap();
        let (title, bytes) = read_import(&path).unwrap();
        assert_eq!(title, "Super Game (USA)");
        assert_eq!(bytes, b"NES\x1a");
        let huge = dir.join("huge.nes");
        fs::File::create(&huge).unwrap().set_len(MAX_ROM as u64 + 1).unwrap();
        assert!(read_import(&huge).unwrap_err().contains("too large"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn playtime_reads_like_a_person_wrote_it() {
        assert_eq!(playtime(0), None);
        assert_eq!(playtime(59), None);
        assert_eq!(playtime(60), Some("1 min played".into()));
        assert_eq!(playtime(3599), Some("59 min played".into()));
        assert_eq!(playtime(3600), Some("1 h 0 min played".into()));
        assert_eq!(playtime(7380), Some("2 h 3 min played".into()));
    }
}
```

Run: `cargo test -p nes-desktop library` Expected: panics on `todo!()`.

- [ ] **Step 2: Implement library.rs**

```rust
//! A game on the shelf and everything it owns on disk. Ported from Library.kt,
//! same folder layout, same index, so the two shells are one design.

use crate::files::{now_millis, read_optional, read_png, write_atomic, write_png};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub const ART_MAX_EDGE: u32 = 1024;
pub const MAX_ROM: usize = 16 * 1024 * 1024;
pub const PROBLEM_LINES: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    pub id: String,
    pub title: String,
    pub added: i64,
    #[serde(default)]
    pub played: i64,
    #[serde(default)]
    pub seconds: i64,
    #[serde(default)]
    pub archived: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Auto,
    Number(u8),
}

#[derive(Clone, Debug)]
pub struct SaveSlot {
    pub number: u8,
    pub time: Option<i64>,
    pub thumbnail: PathBuf,
}

pub struct Library {
    root: PathBuf,
}

impl Library {
    pub fn open(data_dir: &Path) -> Result<Library, String> {
        let root = data_dir.join("library");
        fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
        Ok(Library { root })
    }

    fn index(&self) -> PathBuf {
        self.root.join("index.json")
    }

    fn read(&self) -> Vec<Game> {
        read_optional(&self.index())
            .ok()
            .flatten()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn write(&self, games: &[Game]) -> Result<(), String> {
        let text = serde_json::to_string(games).map_err(|e| e.to_string())?;
        write_atomic(&self.index(), text.as_bytes())
    }

    fn sorted(mut games: Vec<Game>) -> Vec<Game> {
        games.sort_by_key(|g| std::cmp::Reverse(g.played.max(g.added)));
        games
    }

    /// The shelf: most recently played first, falling back to when it was added.
    pub fn games(&self) -> Vec<Game> {
        Self::sorted(self.read().into_iter().filter(|g| !g.archived).collect())
    }

    pub fn archived(&self) -> Vec<Game> {
        Self::sorted(self.read().into_iter().filter(|g| g.archived).collect())
    }

    pub fn find(&self, id: &str) -> Option<Game> {
        self.read().into_iter().find(|g| g.id == id)
    }

    pub fn add(&self, id: &str, title: &str, rom: &[u8]) -> Result<Game, String> {
        let mut games = self.read();
        if let Some(existing) = games.iter().find(|g| g.id == id) {
            return Ok(existing.clone());
        }
        let game = Game { id: id.into(), title: title.into(), added: now_millis(), played: 0, seconds: 0, archived: false };
        write_atomic(&self.rom_path(id), rom)?;
        games.push(game.clone());
        self.write(&games)?;
        Ok(game)
    }

    fn update(&self, id: &str, change: impl FnOnce(&mut Game)) -> Result<(), String> {
        let mut games = self.read();
        if let Some(game) = games.iter_mut().find(|g| g.id == id) {
            change(game);
            self.write(&games)?;
        }
        Ok(())
    }

    pub fn set_archived(&self, id: &str, archived: bool) -> Result<(), String> {
        self.update(id, |g| g.archived = archived)
    }

    /// The one that does not come back: the whole folder and the index row.
    pub fn forget(&self, id: &str) -> Result<(), String> {
        let games: Vec<Game> = self.read().into_iter().filter(|g| g.id != id).collect();
        self.write(&games)?;
        let dir = self.directory(id);
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        Ok(())
    }

    /// Records a finished stretch of play. Re-reads the index rather than
    /// trusting a stale copy.
    pub fn record(&self, id: &str, seconds: i64) -> Result<(), String> {
        self.update(id, |g| {
            g.played = now_millis();
            g.seconds += seconds.max(0);
        })
    }

    pub fn directory(&self, id: &str) -> PathBuf {
        let dir = self.root.join(id);
        let _ = fs::create_dir_all(&dir);
        dir
    }
    pub fn rom_path(&self, id: &str) -> PathBuf {
        self.directory(id).join("game.nes")
    }
    pub fn battery_path(&self, id: &str) -> PathBuf {
        self.directory(id).join("battery.sav")
    }
    pub fn state_path(&self, id: &str, slot: Slot) -> PathBuf {
        self.directory(id).join(match slot {
            Slot::Auto => "auto.state".to_string(),
            Slot::Number(n) => format!("slot-{n}.state"),
        })
    }
    pub fn thumbnail_path(&self, id: &str, slot: Slot) -> PathBuf {
        self.directory(id).join(match slot {
            Slot::Auto => "auto.png".to_string(),
            Slot::Number(n) => format!("slot-{n}.png"),
        })
    }
    pub fn art_path(&self, id: &str) -> PathBuf {
        self.directory(id).join("art.png")
    }

    pub fn slots(&self, id: &str) -> Vec<SaveSlot> {
        (0..10)
            .map(|n| SaveSlot {
                number: n,
                time: fs::metadata(self.state_path(id, Slot::Number(n)))
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64),
                thumbnail: self.thumbnail_path(id, Slot::Number(n)),
            })
            .collect()
    }

    pub fn has_autosave(&self, id: &str) -> bool {
        self.state_path(id, Slot::Auto).exists()
    }

    /// Chosen art, else the last saved moment, else nothing.
    pub fn cover(&self, id: &str) -> Option<PathBuf> {
        [self.art_path(id), self.thumbnail_path(id, Slot::Auto)].into_iter().find(|p| p.exists())
    }

    /// Box art: decoded, shrunk so the longest edge is at most 1024, stored as PNG.
    pub fn set_art(&self, id: &str, image_path: &Path) -> Result<(), String> {
        let image = image::open(image_path).map_err(|e| format!("{}: {e}", image_path.display()))?;
        let image = if image.width().max(image.height()) > ART_MAX_EDGE {
            image.resize(ART_MAX_EDGE, ART_MAX_EDGE, image::imageops::FilterType::Triangle)
        } else {
            image
        };
        let rgba = image.to_rgba8();
        write_png(&self.art_path(id), rgba.width(), rgba.height(), rgba.as_raw())
    }

    pub fn clear_art(&self, id: &str) -> Result<(), String> {
        match fs::remove_file(self.art_path(id)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn problems_path(&self) -> PathBuf {
        self.root.join("problems.log")
    }

    /// The plain sentence goes on screen; the real reason lands here. Bounded,
    /// because a failure that repeats every frame would otherwise fill the disk.
    pub fn log_problem(&self, label: &str, detail: &str) {
        let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let existing = fs::read_to_string(self.problems_path()).unwrap_or_default();
        let mut lines: Vec<&str> = existing.lines().collect();
        if lines.len() >= PROBLEM_LINES {
            lines = lines[lines.len() - (PROBLEM_LINES - 1)..].to_vec();
        }
        let mut text = lines.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&format!("{stamp}  {label} — {detail}"));
        let _ = write_atomic(&self.problems_path(), text.as_bytes());
    }

    pub fn problems(&self) -> Vec<String> {
        let text = fs::read_to_string(self.problems_path()).unwrap_or_default();
        text.lines().rev().map(String::from).collect()
    }
}

/// A file the person picked: the title is its name without the extension,
/// and the bytes are bounded because nothing 16 MB long is a game.
pub fn read_import(path: &Path) -> Result<(String, Vec<u8>), String> {
    let title = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "My game".into());
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(MAX_ROM as u64 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_ROM {
        return Err("This file is too large to be a game".into());
    }
    Ok((title, bytes))
}

pub fn playtime(seconds: i64) -> Option<String> {
    match seconds {
        s if s < 60 => None,
        s if s < 3600 => Some(format!("{} min played", s / 60)),
        s => Some(format!("{} h {} min played", s / 3600, (s % 3600) / 60)),
    }
}

pub fn thumbnail_pixels(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    read_png(path).ok()
}
```

Add `mod library;` to `main.rs`. Remove the `#[allow(dead_code)]` from `mod files;` if everything is now used; otherwise keep it until Task 9.

- [ ] **Step 3: Run tests and clippy**

Run: `cargo test -p nes-desktop library && cargo clippy -p nes-desktop --all-targets --no-deps -- -D warnings`
Expected: 7 pass, clean.

- [ ] **Step 4: Commit**

```bash
git add desktop/src/library.rs desktop/src/main.rs
git commit -m "Add the desktop library: index, game folders, slots, art and problem log"
```

---

### Task 5: Engine and session (headless emulation with saves, rewind and scrub)

**Files:**
- Create: `desktop/src/engine.rs`, `desktop/src/session.rs`
- Modify: `desktop/src/main.rs` (add modules; the old `play` and its focus test move into `session.rs` in Task 6)

**Interfaces:**
- Consumes: `nes_core::{Nes, Buttons, rewind::Rewind}`, `library::{Library, Game, Slot}`, `files::*`, `picture`, `scrub`
- Produces (`engine.rs`):
  - `const REWIND_BUDGET: usize = 64 * 1024 * 1024`
  - `struct Engine` with `new(rom: &[u8]) -> Result<Engine, String>`, `id(&self) -> String` (hash, 16 hex), `identity(&self) -> String`, `frame_rate(&self) -> f64`, `set_palette(&mut self, colours: [u32; 64])` (repaints), `frame(&self) -> &[u8]` (RGBA 256×240), `step(&mut self, p1: Buttons, p2: Buttons)` (one frame, pushes rewind, repaints), `samples(&self) -> &[f32]`, `rewind_step(&mut self) -> bool`, `rewind_depth(&self) -> usize`, `frames_for(&self, seconds: u32) -> usize`, `save_state(&self) -> Vec<u8>`, `load_state(&mut self, bytes: &[u8]) -> Result<(), String>` (clears rewind), `battery_ram(&self) -> Option<&[u8]>`, `load_battery(&mut self, bytes: &[u8]) -> Result<(), String>`, `reset(&mut self) -> Result<(), String>` (fresh machine, battery carried, rewind cleared), `header_notes(&self) -> Vec<&'static str>`
- Produces (`session.rs`):
  - `trait Audio { fn queued_bytes(&self) -> u32; fn queue(&mut self, samples: &[f32]) -> Result<(), String>; fn clear(&mut self); fn pause(&mut self); fn resume(&mut self); fn queued_ms(&self) -> f32 }` and `impl Audio for sdl2::audio::AudioQueue<f32>`
  - `struct Session { pub engine: Engine, pub game: Game, pub paused: bool, pub scrub: i32, ... }`
  - `Session::open(library: &Library, game: Game, audio: Option<Box<dyn Audio>>) -> Result<(Session, Vec<String>), String>` where the `Vec<String>` are plain warnings to show ("This game had to start from an earlier point.")
  - `Session::advance(&mut self, p1: Buttons, p2: Buttons) -> Advance` with `struct Advance { frames: usize, hit_end: bool }`; call once per display tick; internally runs frames per audio budget (or the region frame time when muted), scrub speed, and does the five-second battery flush and pause-transition flush, returning flush errors through `Session::take_error(&mut self) -> Option<String>`
  - `Session::flush_battery(&mut self) -> Result<(), String>`, `Session::save(&mut self, slot: Slot) -> Result<(), String>` (state + battery + PNG thumbnail), `Session::load(&mut self, slot: Slot) -> Result<(), String>`, `Session::reset(&mut self) -> Result<(), String>` (engine reset then autosave), `Session::record_playtime(&mut self)`, `Session::audio_ms(&self) -> f32`, `Session::screenshot(&self, folder: &Path) -> Result<PathBuf, String>`, `Session::close(&mut self)` (autosave + battery + playtime; errors logged through the library)
  - `migrate_flat_saves(data_dir: &Path, library: &Library, id: &str, identity: &str) -> Result<Vec<String>, String>` (moves `<identity>.sav` to `battery.sav`, `<identity>.state` to `slot-0.state`; returns what moved)

- [ ] **Step 1: Write the failing engine tests**

Create `desktop/src/engine.rs` with the interface stubbed and these tests (the NROM builders come from today's tests in `main.rs`):

```rust
#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// NROM with CHR RAM and battery: `INC $00; JMP $8000` style loop.
    pub fn test_rom() -> Vec<u8> {
        let mut rom = vec![0; 16 + 16384];
        rom[..4].copy_from_slice(b"NES\x1a");
        rom[4] = 1;
        rom[6] = 2;
        rom[16..22].copy_from_slice(&[0xe6, 0x00, 0x4c, 0x00, 0x80, 0xea]);
        rom[16 + 0x3ffc..16 + 0x3ffe].copy_from_slice(&[0, 0x80]);
        rom
    }

    #[test]
    fn the_id_is_the_header_hash_android_uses() {
        let engine = Engine::new(&test_rom()).unwrap();
        let nes = Nes::new(&test_rom()).unwrap();
        assert_eq!(engine.id(), format!("{:016x}", nes.bus.cart.header.hash));
        assert_eq!(engine.identity(), format!("{:016x}", nes.bus.cart.header.identity));
        assert_eq!(engine.id().len(), 16);
    }

    #[test]
    fn stepping_paints_rgba_and_grows_the_rewind_chain() {
        let mut engine = Engine::new(&test_rom()).unwrap();
        assert_eq!(engine.frame().len(), 256 * 240 * 4);
        assert_eq!(engine.rewind_depth(), 0);
        engine.step(Buttons(0), Buttons(0));
        assert_eq!(engine.rewind_depth(), 1);
        assert!(!engine.samples().is_empty());
        assert!(engine.frame().iter().skip(3).step_by(4).all(|&a| a == 255));
        assert!(engine.rewind_step());
        assert_eq!(engine.rewind_depth(), 0);
        assert!(!engine.rewind_step());
    }

    #[test]
    fn a_palette_change_repaints_the_paused_frame() {
        let mut engine = Engine::new(&test_rom()).unwrap();
        engine.step(Buttons(0), Buttons(0));
        let before = engine.frame()[0..3].to_vec();
        engine.set_palette([0x123456; 64]);
        assert_eq!(&engine.frame()[0..3], &[0x12, 0x34, 0x56]);
        assert_ne!(before, engine.frame()[0..3]);
    }

    #[test]
    fn save_state_restores_the_machine_and_replays() {
        let mut engine = Engine::new(&test_rom()).unwrap();
        engine.step(Buttons(0), Buttons(0));
        let saved = engine.save_state();
        engine.step(Buttons(0), Buttons(0));
        let expected = engine.save_state();
        engine.load_state(&saved).unwrap();
        assert_eq!(engine.rewind_depth(), 0);
        engine.step(Buttons(0), Buttons(0));
        assert_eq!(engine.save_state(), expected);
        assert!(engine.load_state(b"garbage").is_err());
    }

    #[test]
    fn reset_keeps_battery_ram_and_clears_the_chain() {
        use nes_core::cpu::Bus;
        let mut engine = Engine::new(&test_rom()).unwrap();
        engine.nes.bus.write(0x6000, 0x5a);
        engine.step(Buttons(0), Buttons(0));
        engine.reset().unwrap();
        assert_eq!(engine.rewind_depth(), 0);
        assert_eq!(engine.battery_ram().unwrap()[0], 0x5a);
    }

    #[test]
    fn frames_for_uses_the_region_rate() {
        let engine = Engine::new(&test_rom()).unwrap();
        assert_eq!(engine.frames_for(5), (5.0 * engine.frame_rate()) as usize);
        assert!(engine.frames_for(15) > 700);
    }
}
```

- [ ] **Step 2: Implement engine.rs**

```rust
//! The machine, its rewind chain and the colours its framebuffer means, with
//! nothing about windows, audio devices or files. The counterpart of the JNI
//! layer in native/src/lib.rs, and shaped the same way on purpose.

use crate::picture::{HEIGHT, WIDTH};
use nes_core::{rewind::Rewind, Buttons, Nes};

/// Bounded in bytes rather than frames: see `nes_core::rewind`.
pub const REWIND_BUDGET: usize = 64 * 1024 * 1024;

pub struct Engine {
    pub(crate) nes: Nes,
    rewind: Rewind,
    rom: Vec<u8>,
    palette: [u32; 64],
    frame: Vec<u8>,
}

impl Engine {
    pub fn new(rom: &[u8]) -> Result<Engine, String> {
        let nes = Nes::new(rom).map_err(|e| e.to_string())?;
        let mut engine = Engine {
            nes,
            rewind: Rewind::new(REWIND_BUDGET),
            rom: rom.to_vec(),
            palette: crate::palette::PALETTE,
            frame: vec![0; WIDTH * HEIGHT * 4],
        };
        engine.repaint();
        Ok(engine)
    }

    pub fn id(&self) -> String {
        format!("{:016x}", self.nes.bus.cart.header.hash)
    }
    pub fn identity(&self) -> String {
        format!("{:016x}", self.nes.bus.cart.header.identity)
    }
    pub fn frame_rate(&self) -> f64 {
        self.nes.bus.cart.header.region.frame_rate()
    }
    pub fn frames_for(&self, seconds: u32) -> usize {
        (seconds as f64 * self.frame_rate()) as usize
    }
    pub fn header_notes(&self) -> Vec<&'static str> {
        self.nes.bus.cart.header.fixes.labels().collect()
    }

    fn repaint(&mut self) {
        for (i, &index) in self.nes.framebuffer().iter().enumerate() {
            let rgb = self.palette[(index & 63) as usize];
            let at = i * 4;
            self.frame[at] = (rgb >> 16) as u8;
            self.frame[at + 1] = (rgb >> 8) as u8;
            self.frame[at + 2] = rgb as u8;
            self.frame[at + 3] = 255;
        }
    }

    pub fn set_palette(&mut self, colours: [u32; 64]) {
        self.palette = colours;
        self.repaint();
    }

    pub fn frame(&self) -> &[u8] {
        &self.frame
    }

    pub fn step(&mut self, p1: Buttons, p2: Buttons) {
        self.nes.set_buttons(0, p1);
        self.nes.set_buttons(1, p2);
        self.nes.step_frame();
        self.rewind.push(&self.nes.save_state());
        self.repaint();
    }

    pub fn samples(&self) -> &[f32] {
        self.nes.bus.apu.samples()
    }

    /// One frame back. False when the chain is dry; the frame is repainted either way.
    pub fn rewind_step(&mut self) -> bool {
        let Some(state) = self.rewind.pop() else {
            return false;
        };
        let restored = self.nes.load_state(state).is_ok();
        self.repaint();
        restored
    }
    pub fn rewind_depth(&self) -> usize {
        self.rewind.depth()
    }

    pub fn save_state(&self) -> Vec<u8> {
        self.nes.save_state()
    }
    /// A restored state leads to a timeline that was not played, so the chain goes.
    pub fn load_state(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.nes.load_state(bytes).map_err(|e| e.to_string())?;
        self.rewind.clear();
        self.repaint();
        Ok(())
    }
    pub fn battery_ram(&self) -> Option<&[u8]> {
        self.nes.battery_ram()
    }
    pub fn load_battery(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.nes.load_battery_ram(bytes).map_err(|e| e.to_string())
    }

    /// A fresh machine from the same ROM with battery RAM carried across,
    /// which is what a console's reset button did.
    pub fn reset(&mut self) -> Result<(), String> {
        let mut fresh = Nes::new(&self.rom).map_err(|e| e.to_string())?;
        if let Some(battery) = self.nes.battery_ram() {
            fresh.load_battery_ram(battery).map_err(|e| e.to_string())?;
        }
        self.nes = fresh;
        self.rewind.clear();
        self.repaint();
        Ok(())
    }
}
```

Check `Rewind::pop` returns `Option<&[u8]>` (it does, `core/src/rewind.rs:76`); if the borrow of `self.rewind` conflicts with `self.nes.load_state`, copy the popped state into a `Vec<u8>` first.

Run: `cargo test -p nes-desktop engine` Expected: 6 pass.

- [ ] **Step 3: Write the failing session tests**

Create `desktop/src/session.rs` with the interface stubbed and these tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::test_rom;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulia-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Records what an audio device would have been asked to do.
    #[derive(Default)]
    struct FakeAudio {
        queued: u32,
        cleared: usize,
        paused: bool,
    }
    impl Audio for FakeAudio {
        fn queued_bytes(&self) -> u32 { self.queued }
        fn queue(&mut self, samples: &[f32]) -> Result<(), String> { self.queued += samples.len() as u32 * 4; Ok(()) }
        fn clear(&mut self) { self.queued = 0; self.cleared += 1; }
        fn pause(&mut self) { self.paused = true; }
        fn resume(&mut self) { self.paused = false; }
        fn queued_ms(&self) -> f32 { self.queued as f32 / 4.0 / 48.0 }
    }

    fn library_with_game(dir: &Path) -> (Library, Game) {
        let library = Library::open(dir).unwrap();
        let engine = Engine::new(&test_rom()).unwrap();
        let game = library.add(&engine.id(), "Test", &test_rom()).unwrap();
        (library, game)
    }

    #[test]
    fn opening_restores_battery_then_autosave_and_reports_a_bad_one() {
        let dir = temp_dir("session-open");
        let (library, game) = library_with_game(&dir);
        let (mut session, warnings) = Session::open(&library, game.clone(), None).unwrap();
        assert!(warnings.is_empty());
        use nes_core::cpu::Bus;
        session.engine.nes.bus.write(0x6000, 0x5a);
        session.advance(Buttons(0), Buttons(0));
        session.close();
        assert!(library.has_autosave(&game.id));
        assert_eq!(fs::read(library.battery_path(&game.id)).unwrap()[0], 0x5a);
        let (session, warnings) = Session::open(&library, game.clone(), None).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(session.engine.battery_ram().unwrap()[0], 0x5a);
        fs::write(library.state_path(&game.id, Slot::Auto), b"corrupt").unwrap();
        let (session, warnings) = Session::open(&library, game.clone(), None).unwrap();
        assert_eq!(warnings, vec!["This game had to start from an earlier point.".to_string()]);
        assert!(session.paused);
        assert!(library.problems()[0].contains("auto.state"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn advance_runs_frames_until_the_audio_queue_holds_two_frames() {
        let dir = temp_dir("session-audio");
        let (library, game) = library_with_game(&dir);
        let (mut session, _) = Session::open(&library, game, Some(Box::new(FakeAudio::default()))).unwrap();
        let first = session.advance(Buttons(0), Buttons(0));
        assert!(first.frames >= 1 && first.frames <= 3, "{}", first.frames);
        let second = session.advance(Buttons(0), Buttons(0));
        assert_eq!(second.frames, 0, "queue already full");
        assert!(session.audio_ms() > 20.0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scrubbing_steps_many_frames_or_rewinds_and_reports_the_end() {
        let dir = temp_dir("session-scrub");
        let (library, game) = library_with_game(&dir);
        let (mut session, _) = Session::open(&library, game, None).unwrap();
        session.scrub = 4;
        assert_eq!(session.advance(Buttons(0), Buttons(0)).frames, 4);
        session.scrub = -2;
        let back = session.advance(Buttons(0), Buttons(0));
        assert_eq!(back.frames, 2);
        assert!(!back.hit_end);
        assert_eq!(session.engine.rewind_depth(), 2);
        session.scrub = -8;
        assert!(session.advance(Buttons(0), Buttons(0)).hit_end);
        assert_eq!(session.engine.rewind_depth(), 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn pausing_flushes_battery_and_silences_audio() {
        let dir = temp_dir("session-pause");
        let (library, game) = library_with_game(&dir);
        let (mut session, _) = Session::open(&library, game.clone(), Some(Box::new(FakeAudio::default()))).unwrap();
        use nes_core::cpu::Bus;
        session.engine.nes.bus.write(0x6000, 0x5a);
        session.paused = true;
        let paused = session.advance(Buttons(0), Buttons(0));
        assert_eq!(paused.frames, 0);
        assert_eq!(fs::read(library.battery_path(&game.id)).unwrap()[0], 0x5a);
        assert!(session.take_error().is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_blocked_battery_save_is_reported_and_retried_without_closing() {
        let dir = temp_dir("session-blocked");
        let (library, game) = library_with_game(&dir);
        let (mut session, _) = Session::open(&library, game.clone(), None).unwrap();
        let battery = library.battery_path(&game.id);
        let blocked = battery.with_extension(format!("tmp-{}", std::process::id()));
        fs::write(&blocked, b"conflict").unwrap();
        assert!(session.flush_battery().unwrap_err().contains(&blocked.display().to_string()));
        fs::remove_file(&blocked).unwrap();
        session.flush_battery().unwrap();
        assert!(battery.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn slots_save_state_battery_and_thumbnail_and_load_back() {
        let dir = temp_dir("session-slots");
        let (library, game) = library_with_game(&dir);
        let (mut session, _) = Session::open(&library, game.clone(), None).unwrap();
        session.advance(Buttons(0), Buttons(0));
        session.save(Slot::Number(3)).unwrap();
        let slots = library.slots(&game.id);
        assert!(slots[3].time.is_some());
        let (w, h, _) = crate::files::read_png(&slots[3].thumbnail).unwrap();
        assert_eq!((w, h), (256, 240));
        let expected = session.engine.save_state();
        session.scrub = 3;
        session.advance(Buttons(0), Buttons(0));
        session.load(Slot::Number(3)).unwrap();
        assert_eq!(session.engine.save_state(), expected);
        assert_eq!(session.engine.rewind_depth(), 0);
        assert!(session.load(Slot::Number(7)).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reset_replaces_the_autosave_with_the_new_start() {
        let dir = temp_dir("session-reset");
        let (library, game) = library_with_game(&dir);
        let (mut session, _) = Session::open(&library, game.clone(), None).unwrap();
        session.scrub = 5;
        session.advance(Buttons(0), Buttons(0));
        session.reset().unwrap();
        assert_eq!(session.engine.rewind_depth(), 0);
        assert!(library.has_autosave(&game.id));
        assert!(!session.paused);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn flat_saves_from_the_first_desktop_release_migrate_on_open() {
        let dir = temp_dir("session-migrate");
        let (library, game) = library_with_game(&dir);
        let engine = Engine::new(&test_rom()).unwrap();
        fs::write(dir.join(format!("{}.sav", engine.identity())), vec![0x5a; 8192]).unwrap();
        let state = engine.save_state();
        fs::write(dir.join(format!("{}.state", engine.identity())), &state).unwrap();
        let (session, warnings) = Session::open(&library, game.clone(), None).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(session.engine.battery_ram().unwrap()[0], 0x5a);
        assert_eq!(fs::read(library.state_path(&game.id, Slot::Number(0))).unwrap(), state);
        assert!(!dir.join(format!("{}.sav", engine.identity())).exists());
        assert!(library.problems().iter().any(|l| l.contains("moved")));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn screenshots_are_the_raw_frame_named_after_the_game() {
        let dir = temp_dir("session-shot");
        let (library, game) = library_with_game(&dir);
        let (session, _) = Session::open(&library, game, None).unwrap();
        let path = session.screenshot(&dir.join("Pictures")).unwrap();
        assert!(path.file_name().unwrap().to_string_lossy().starts_with("Test "));
        let (w, h, _) = crate::files::read_png(&path).unwrap();
        assert_eq!((w, h), (256, 240));
        fs::remove_dir_all(dir).unwrap();
    }
}
```

- [ ] **Step 4: Implement session.rs**

```rust
//! One open game: the engine, the files it saves to, the audio queue that
//! clocks it and the timers that flush it. Used by both the window and the
//! headless `--frames` run, so nothing here touches a display.

use crate::engine::Engine;
use crate::files::{read_optional, write_atomic, write_png};
use crate::library::{Game, Library, Slot};
use crate::picture::{HEIGHT, WIDTH};
use nes_core::Buttons;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub trait Audio {
    fn queued_bytes(&self) -> u32;
    fn queue(&mut self, samples: &[f32]) -> Result<(), String>;
    fn clear(&mut self);
    fn pause(&mut self);
    fn resume(&mut self);
    fn queued_ms(&self) -> f32;
}

impl Audio for sdl2::audio::AudioQueue<f32> {
    fn queued_bytes(&self) -> u32 { self.size() }
    fn queue(&mut self, samples: &[f32]) -> Result<(), String> { self.queue_audio(samples) }
    fn clear(&mut self) { sdl2::audio::AudioQueue::clear(self) }
    fn pause(&mut self) { sdl2::audio::AudioQueue::pause(self) }
    fn resume(&mut self) { sdl2::audio::AudioQueue::resume(self) }
    fn queued_ms(&self) -> f32 { self.size() as f32 / 4.0 / 48.0 }
}

pub struct Advance {
    pub frames: usize,
    pub hit_end: bool,
}

const FLUSH_EVERY: Duration = Duration::from_secs(5);
/// Frames run in one tick when the audio queue has drained, so a stall
/// never turns into a burst.
const MAX_CATCH_UP: usize = 3;
pub const START_EARLIER: &str = "This game had to start from an earlier point.";

pub struct Session {
    pub engine: Engine,
    pub game: Game,
    pub paused: bool,
    pub scrub: i32,
    battery: PathBuf,
    library_root: PathBuf,
    audio: Option<Box<dyn Audio>>,
    target_bytes: u32,
    frame_time: Duration,
    deadline: Instant,
    last_flush: Instant,
    was_paused: bool,
    played_since: Instant,
    error: Option<String>,
}

impl Session {
    pub fn open(library: &Library, game: Game, audio: Option<Box<dyn Audio>>) -> Result<(Session, Vec<String>), String> {
        let rom = fs::read(library.rom_path(&game.id)).map_err(|e| format!("{}: {e}", library.rom_path(&game.id).display()))?;
        let mut engine = Engine::new(&rom)?;
        let data_dir = library.data_dir();
        for note in migrate_flat_saves(&data_dir, library, &game.id, &engine.identity())? {
            library.log_problem("Moved an older save", &note);
        }
        for note in engine.header_notes() {
            library.log_problem(&format!("{}: header corrected", game.title), note);
        }
        let mut warnings = Vec::new();
        let battery = library.battery_path(&game.id);
        let mut paused = false;
        if engine.battery_ram().is_some() {
            match read_optional(&battery) {
                Ok(Some(bytes)) => {
                    if let Err(e) = engine.load_battery(&bytes) {
                        library.log_problem(&game.title, &format!("{}: {e}", battery.display()));
                        warnings.push(START_EARLIER.to_string());
                        paused = true;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    library.log_problem(&game.title, &e);
                    warnings.push(START_EARLIER.to_string());
                    paused = true;
                }
            }
        }
        let auto = library.state_path(&game.id, Slot::Auto);
        if let Ok(Some(bytes)) = read_optional(&auto) {
            if let Err(e) = engine.load_state(&bytes) {
                library.log_problem(&game.title, &format!("{}: {e}", auto.display()));
                if warnings.is_empty() {
                    warnings.push(START_EARLIER.to_string());
                }
                paused = true;
            }
        }
        let frame_time = Duration::from_secs_f64(1.0 / engine.frame_rate());
        let target_bytes = (48_000.0 * frame_time.as_secs_f64() * 2.0).ceil() as u32 * 4;
        let now = Instant::now();
        Ok((
            Session {
                engine,
                game,
                paused,
                scrub: 0,
                battery,
                library_root: library.root().to_path_buf(),
                audio,
                target_bytes,
                frame_time,
                deadline: now,
                last_flush: now,
                was_paused: false,
                played_since: now,
                error: None,
            },
            warnings,
        ))
    }

    fn library(&self) -> Library {
        Library::from_root(self.library_root.clone())
    }

    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    pub fn audio_ms(&self) -> f32 {
        self.audio.as_ref().map_or(0.0, |a| a.queued_ms())
    }

    /// Once per display tick. Runs what the clock allows and keeps the saves
    /// current; never blocks.
    pub fn advance(&mut self, p1: Buttons, p2: Buttons) -> Advance {
        let is_paused = self.paused;
        // Flush when putting a game aside, and every few seconds otherwise.
        if (is_paused && !self.was_paused) || self.last_flush.elapsed() >= FLUSH_EVERY {
            if let Err(e) = self.flush_battery() {
                // A temporarily unavailable save directory must not close the
                // game and discard the only remaining copy in RAM.
                eprintln!("{e}");
                self.error = Some(e);
            }
            self.last_flush = Instant::now();
        }
        self.was_paused = is_paused;
        let mut result = Advance { frames: 0, hit_end: false };
        if is_paused {
            if let Some(a) = &mut self.audio {
                a.pause();
                a.clear();
            }
            self.deadline = Instant::now();
            return result;
        }
        if self.scrub < 0 {
            if let Some(a) = &mut self.audio {
                a.clear();
            }
            for _ in 0..-self.scrub {
                if !self.engine.rewind_step() {
                    result.hit_end = true;
                    break;
                }
                result.frames += 1;
            }
            self.deadline = Instant::now();
            return result;
        }
        if self.scrub > 0 {
            for _ in 0..self.scrub {
                self.engine.step(p1, p2);
                result.frames += 1;
            }
            // Only the last frame is heard, and only if there is room for it.
            if let Some(a) = &mut self.audio {
                if a.queued_bytes() < self.target_bytes {
                    let _ = a.queue(self.engine.samples());
                    a.resume();
                }
            }
            self.deadline = Instant::now();
            return result;
        }
        match &mut self.audio {
            Some(a) => {
                while a.queued_bytes() < self.target_bytes && result.frames < MAX_CATCH_UP {
                    self.engine.step(p1, p2);
                    result.frames += 1;
                    if let Err(e) = a.queue(self.engine.samples()) {
                        self.error = Some(e);
                    }
                    a.resume();
                }
            }
            None => {
                let now = Instant::now();
                while now >= self.deadline && result.frames < MAX_CATCH_UP {
                    self.engine.step(p1, p2);
                    result.frames += 1;
                    self.deadline += self.frame_time;
                }
                if now.saturating_duration_since(self.deadline) > self.frame_time {
                    self.deadline = now;
                }
            }
        }
        result
    }

    /// How long until the next frame is due when there is no audio device to
    /// wait on; the headless loop sleeps this.
    pub fn until_due(&self) -> Duration {
        if self.audio.is_some() || self.paused || self.scrub != 0 {
            Duration::from_millis(1)
        } else {
            self.deadline.saturating_duration_since(Instant::now()).min(self.frame_time)
        }
    }

    pub fn flush_battery(&mut self) -> Result<(), String> {
        if let Some(bytes) = self.engine.battery_ram() {
            write_atomic(&self.battery, bytes)?;
        }
        Ok(())
    }

    /// The state, the battery RAM if any and a thumbnail, each atomically.
    pub fn save(&mut self, slot: Slot) -> Result<(), String> {
        let library = self.library();
        write_atomic(&library.state_path(&self.game.id, slot), &self.engine.save_state())?;
        self.flush_battery()?;
        write_png(&library.thumbnail_path(&self.game.id, slot), WIDTH as u32, HEIGHT as u32, self.engine.frame())
    }

    pub fn load(&mut self, slot: Slot) -> Result<(), String> {
        let path = self.library().state_path(&self.game.id, slot);
        let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        self.engine.load_state(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        if let Some(a) = &mut self.audio {
            a.clear();
        }
        Ok(())
    }

    /// Restart from the beginning with battery RAM kept; the autosave is
    /// replaced by the new start, never restored.
    pub fn reset(&mut self) -> Result<(), String> {
        self.scrub = 0;
        self.engine.reset()?;
        if let Some(a) = &mut self.audio {
            a.clear();
        }
        self.paused = false;
        self.save(Slot::Auto)
    }

    pub fn record_playtime(&mut self) {
        let seconds = self.played_since.elapsed().as_secs() as i64;
        self.played_since = Instant::now();
        if let Err(e) = self.library().record(&self.game.id, seconds) {
            eprintln!("{e}");
        }
    }

    pub fn screenshot(&self, folder: &Path) -> Result<PathBuf, String> {
        fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        let title: String = self
            .game
            .title
            .chars()
            .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let stamp = chrono::Local::now().format("%Y-%m-%d %H.%M.%S");
        let path = folder.join(format!("{title} {stamp}.png"));
        write_png(&path, WIDTH as u32, HEIGHT as u32, self.engine.frame())?;
        Ok(path)
    }

    /// Autosave, battery and playtime on the way out. Errors are logged, not returned.
    pub fn close(&mut self) {
        let library = self.library();
        if let Err(e) = self.save(Slot::Auto) {
            library.log_problem(&self.game.title, &e);
        }
        self.record_playtime();
    }
}

/// The first desktop release keyed `<identity>.sav` and `<identity>.state` at
/// the data directory root. Move them into the game's folder once.
pub fn migrate_flat_saves(data_dir: &Path, library: &Library, id: &str, identity: &str) -> Result<Vec<String>, String> {
    let mut moved = Vec::new();
    for (old, new) in [
        (data_dir.join(format!("{identity}.sav")), library.battery_path(id)),
        (data_dir.join(format!("{identity}.state")), library.state_path(id, Slot::Number(0))),
    ] {
        if old.exists() && !new.exists() {
            fs::rename(&old, &new).map_err(|e| format!("{}: {e}", old.display()))?;
            moved.push(format!("moved {} to {}", old.display(), new.display()));
        }
    }
    Ok(moved)
}
```

This needs two small additions to `library.rs`: `pub fn root(&self) -> &Path { &self.root }`, `pub fn data_dir(&self) -> PathBuf { self.root.parent().unwrap().to_path_buf() }` and `pub fn from_root(root: PathBuf) -> Library { Library { root } }`.

Add `mod engine; mod session;` to `main.rs`.

- [ ] **Step 5: Run tests and clippy**

Run: `cargo test -p nes-desktop && cargo clippy -p nes-desktop --all-targets --no-deps -- -D warnings`
Expected: all pass. (The `impl Audio for AudioQueue<f32>` is compiled but only constructed in Task 6.)

- [ ] **Step 6: Commit**

```bash
git add desktop/src/engine.rs desktop/src/session.rs desktop/src/library.rs desktop/src/main.rs
git commit -m "Add the desktop engine and session: rewind, scrub, slots, autosave, migration"
```

---

### Task 6: The command line and the headless run

**Files:**
- Modify: `desktop/src/main.rs` (rewrite: options, headless, launch)
- Modify: `scripts/check-desktop.py`

**Interfaces:**
- Consumes: `session::{Session, Audio}`, `library::{Library, read_import}`, `engine::Engine`, `files::default_data_dir`
- Produces: `struct Options { rom: Option<PathBuf>, data_dir: PathBuf, mute: bool, frames: Option<u64> }`, `fn options(args) -> Result<Option<Options>, String>`, `fn import(library: &Library, path: &Path) -> Result<Game, String>` (reads, validates through `Engine::new`, adds by hash), `fn open_audio(sdl: &sdl2::Sdl) -> Result<Box<dyn Audio>, String>`, `fn headless(options: &Options, frames: u64) -> Result<(), String>`. The shell entry point `shell::run(options: Options) -> Result<(), String>` is created as a stub in this task (`Err("The window is not built yet; use --frames")`) and filled in Task 9.

- [ ] **Step 1: Write the failing options tests**

In `main.rs` tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<Options>, String> {
        options(args.iter().map(OsString::from))
    }

    #[test]
    fn no_arguments_opens_the_shelf() {
        let o = parse(&[]).unwrap().unwrap();
        assert!(o.rom.is_none() && !o.mute && o.frames.is_none());
    }

    #[test]
    fn a_rom_path_and_options_parse() {
        let o = parse(&["game.nes", "--mute", "--data-dir", "/tmp/x", "--frames", "6"]).unwrap().unwrap();
        assert_eq!(o.rom.as_deref(), Some(Path::new("game.nes")));
        assert!(o.mute);
        assert_eq!(o.data_dir, PathBuf::from("/tmp/x"));
        assert_eq!(o.frames, Some(6));
        let alias = parse(&["--save-dir", "/tmp/y"]).unwrap().unwrap();
        assert_eq!(alias.data_dir, PathBuf::from("/tmp/y"));
    }

    #[test]
    fn bad_arguments_are_refused() {
        assert!(parse(&["--frames", "0"]).is_err());
        assert!(parse(&["--frames"]).is_err());
        assert!(parse(&["--bogus"]).is_err());
        assert!(parse(&["a.nes", "b.nes"]).is_err());
        assert!(parse(&["--frames", "5"]).unwrap().is_some());
        assert!(headless(&parse(&["--frames", "5", "--mute"]).unwrap().unwrap(), 5).unwrap_err().contains("ROM"));
    }

    #[test]
    fn help_and_list_drivers_print_and_exit() {
        assert!(parse(&["--help"]).unwrap().is_none());
    }
}
```

- [ ] **Step 2: Rewrite main.rs**

```rust
mod engine;
mod files;
mod library;
mod palette;
mod picture;
mod scrub;
mod session;
mod shell;
mod settings;

use library::{Game, Library};
use session::{Audio, Session};
use std::{ffi::OsString, path::{Path, PathBuf}, process::ExitCode, time::Duration};

const HELP: &str = "Emulia desktop
Usage: nes-desktop [rom.nes] [--data-dir <directory>] [--mute] [--frames <count>]
With no ROM the shelf opens. A ROM path is added to the shelf and opened.
Arrows: move   Z: B   X: A   Enter: Start   Right Shift: Select
Escape or Space: pause menu   F11: full screen   F5 / F8: save / load slot 1
Hold . or , to fast-forward or rewind (Shift for faster); Backspace jumps back 5 s (Shift: 15 s)
--frames runs a finite number of frames without a window (also useful with SDL dummy drivers).
--list-drivers lists the compiled SDL video and audio backends.
--save-dir is accepted as an alias for --data-dir.";

pub struct Options {
    pub rom: Option<PathBuf>,
    pub data_dir: PathBuf,
    pub mute: bool,
    pub frames: Option<u64>,
}

fn options(args: impl Iterator<Item = OsString>) -> Result<Option<Options>, String> {
    let mut args = args.peekable();
    let (mut rom, mut data_dir, mut mute, mut frames) = (None, None, false, None);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => {
                println!("{HELP}");
                return Ok(None);
            }
            Some("--list-drivers") => {
                println!("Video: {}", sdl2::video::drivers().collect::<Vec<_>>().join(", "));
                println!("Audio: {}", sdl2::audio::drivers().collect::<Vec<_>>().join(", "));
                return Ok(None);
            }
            Some("--mute") => mute = true,
            Some("--data-dir" | "--save-dir") => {
                data_dir = Some(PathBuf::from(args.next().ok_or("--data-dir needs a directory")?))
            }
            Some("--frames") => {
                let n = args
                    .next()
                    .and_then(|n| n.to_str().and_then(|s| s.parse::<u64>().ok()))
                    .filter(|&n| n > 0)
                    .ok_or("--frames needs a positive integer")?;
                frames = Some(n);
            }
            Some(s) if s.starts_with('-') => return Err(format!("Unknown option: {s}")),
            _ if rom.is_none() => rom = Some(PathBuf::from(arg)),
            _ => return Err("Only one ROM can be opened at a time".into()),
        }
    }
    Ok(Some(Options {
        rom,
        data_dir: match data_dir {
            Some(p) => p,
            None => files::default_data_dir()?,
        },
        mute,
        frames,
    }))
}

/// Read, validate and shelve a ROM file. The same ROM twice is one game.
pub fn import(library: &Library, path: &Path) -> Result<Game, String> {
    let (title, bytes) = library::read_import(path)?;
    let engine = engine::Engine::new(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    library.add(&engine.id(), &title, &bytes)
}

pub fn open_audio(sdl: &sdl2::Sdl) -> Result<Box<dyn Audio>, String> {
    let subsystem = sdl.audio()?;
    let queue = subsystem
        .open_queue::<f32, _>(
            None,
            &sdl2::audio::AudioSpecDesired { freq: Some(48_000), channels: Some(1), samples: Some(512) },
        )
        .map_err(|e| format!("Cannot open audio: {e}. Use --mute to play without sound."))?;
    Ok(Box::new(queue))
}

/// The engine, the saves and the audio queue with no window: what CI can run
/// under SDL's dummy drivers, and what `--frames` means.
fn headless(options: &Options, frames: u64) -> Result<(), String> {
    let rom = options.rom.as_ref().ok_or("--frames needs a ROM path")?;
    std::fs::create_dir_all(&options.data_dir).map_err(|e| format!("{}: {e}", options.data_dir.display()))?;
    let library = Library::open(&options.data_dir)?;
    let game = import(&library, rom)?;
    let sdl = sdl2::init()?;
    let audio = if options.mute { None } else { Some(open_audio(&sdl)?) };
    let (mut session, warnings) = Session::open(&library, game, audio)?;
    for warning in warnings {
        eprintln!("{warning}");
    }
    session.paused = false;
    println!("{HELP}\nData: {}", options.data_dir.display());
    let mut done = 0;
    while done < frames {
        done += session.advance(nes_core::Buttons(0), nes_core::Buttons(0)).frames as u64;
        if let Some(e) = session.take_error() {
            eprintln!("{e}");
        }
        std::thread::sleep(session.until_due().max(Duration::from_millis(1)));
    }
    session.flush_battery()?;
    Ok(())
}

fn run() -> Result<(), String> {
    let Some(options) = options(std::env::args_os().skip(1))? else {
        return Ok(());
    };
    if let Some(frames) = options.frames {
        return headless(&options, frames);
    }
    shell::run(options)
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Emulia: {e}");
            ExitCode::FAILURE
        }
    }
}
```

Create `desktop/src/shell.rs` for now as:

```rust
//! The window. Built in the following tasks.
use crate::Options;

pub fn run(_options: Options) -> Result<(), String> {
    Err("The window is not built yet; use --frames".into())
}
```

Delete the old `play` function, its `HELP` and the focus-flush test from `main.rs` (its behaviour is covered by `session::tests::pausing_flushes_battery_and_silences_audio`, and the shell will call `session.paused = true` on focus loss in Task 9).

Note on the headless loop's `until_due` in the test: `advance` returns `frames` 0 while the queue is full; the loop sleeps 1 ms and tries again, so a 6-frame run finishes in roughly 100 ms with audio and 6 frame times without.

The headless run must not exit on a bad battery save: `Session::open` logs and reports through warnings; `--frames` with a malformed `battery.sav` must exit non-zero, because the smoke test expects `run(command, success=False)` and an untouched file. Add to `headless`: after `Session::open`, if `!warnings.is_empty()` return `Err(warnings.join(" "))`. (`Session::open` already leaves the file intact.)

- [ ] **Step 3: Update the smoke test**

In `scripts/check-desktop.py`:
- Replace `save, = saves.glob("*.sav")` with `save, = saves.rglob("battery.sav")` and after it add:

```python
        index = json.loads((saves / "library" / "index.json").read_text())
        assert len(index) == 1 and index[0]["title"] == "test game", index
        assert (save.parent / "game.nes").read_bytes() == rom
```

- After `run(command + ["--mute"])` add `assert len(json.loads((saves / "library" / "index.json").read_text())) == 1, "a second import must dedupe"`.
- Add `import json` at the top.
- Update the final print to `"Desktop smoke passed: audio, muted pacing, library import, SRAM persistence, autosave recovery and invalid inputs"`.
- The invalid-ROM case: `path.write_bytes(b"not a ROM")` then `run(command, success=False)` still holds because `import` fails.
- The `--frames 0` case still fails at option parsing.

- [ ] **Step 4: Build, test, smoke**

Run:
```bash
cargo test -p nes-desktop
cargo build --release -p nes-desktop
python3 scripts/check-desktop.py target/release/nes-desktop --require-native-drivers
cargo clippy -p nes-desktop --all-targets --no-deps -- -D warnings
```
Expected: tests pass; the smoke test prints the drivers and the "passed" line.

- [ ] **Step 5: Commit**

```bash
git add desktop/src/main.rs desktop/src/shell.rs scripts/check-desktop.py
git commit -m "Give the desktop player a library-backed headless mode and the new command line"
```

---

### Task 7: Input: keyboard, controllers, profiles, time control and the wizard

**Files:**
- Create: `desktop/src/input.rs`
- Modify: `desktop/src/main.rs` (add `mod input;`)

**Interfaces:**
- Consumes: `settings::{Profile, Profiles}`, `nes_core::Buttons`, `sdl2::keyboard::Scancode`, `sdl2::controller::{Button, Axis, GameController}`
- Produces:
  - `enum NesButton { A, B, Select, Start }` with `bit(self) -> u8` (1, 2, 4, 8), `label(self) -> &'static str` ("A", "B", "Select", "Start"), `NesButton::ORDER: [NesButton; 4]`
  - `const TIME_SLOW: i32 = 2`, `const TIME_FAST: i32 = 6`
  - `fn keyboard_default() -> Profile` (X, Z, Right Shift, Return), `fn controller_default(name: &str) -> Profile` (a = "b", b = "a", select = "back", start = "start")
  - `struct Pad { pub instance: u32, pub guid: String, pub name: String, pub port: usize, controller: GameController, buttons: u8, axes: u8, time: i32 }`
  - `struct Input { profiles: Profiles, keys: HashSet<Scancode>, pads: Vec<Pad>, dirty_profiles: bool }`:
    `new(profiles: Profiles) -> Input`, `profiles(&self) -> &Profiles`, `set_profile(&mut self, key: String, profile: Profile)`, `take_dirty(&mut self) -> bool`
    `key(&mut self, scancode: Scancode, pressed: bool)`, `clear(&mut self)`
    `pad_added(&mut self, controller: GameController, guid: String) -> Option<usize>` (port, or None when both ports are taken), `pad_removed(&mut self, instance: u32) -> Option<String>` (name, when it was on a port), `pad_button(&mut self, instance: u32, button: Button, pressed: bool)`, `pad_axis(&mut self, instance: u32, axis: Axis, value: i16)`
    `buttons(&self) -> (Buttons, Buttons)`, `time_speed(&self) -> i32`, `jump_back(&mut self, scancode: Scancode, shift: bool) -> Option<u32>` (5 or 15 on Backspace press)
    `is_start(&self, instance: Option<u32>, physical: &str) -> bool` (through the saved profile; instance None = keyboard)
  - `pub fn keyboard_bits(profile: &Profile, keys: &HashSet<Scancode>) -> u8` and `pub fn pad_bits(profile: &Profile, pressed: &HashSet<String>, axes: u8) -> u8` as pure helpers
  - `struct Wizard { device: Option<(String, String)>, captured: Vec<(NesButton, String)> }` with `new() -> Wizard`, `step(&self) -> usize` (0..4), `device_name(&self) -> Option<&str>`, `press(&mut self, device_key: &str, device_name: &str, physical: &str) -> WizardEvent` and `enum WizardEvent { Advanced, Rejected, Done(String, Profile) }`

Rules ported from Android: the keyboard's physical names are `Scancode::name()`; a controller's are `Button::string()`; only presses count (the caller filters repeats); all four presses must come from the device that pressed first; a physical already captured is rejected.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nes_button_bits_and_order_match_the_core() {
        assert_eq!(NesButton::ORDER.map(|b| b.bit()), [1, 2, 4, 8]);
        assert_eq!(NesButton::ORDER.map(|b| b.label()), ["A", "B", "Select", "Start"]);
    }

    #[test]
    fn the_default_keyboard_profile_is_the_first_release_layout() {
        let p = keyboard_default();
        assert_eq!((p.a.as_str(), p.b.as_str(), p.select.as_str(), p.start.as_str()), ("X", "Z", "Right Shift", "Return"));
        let mut keys = HashSet::new();
        keys.insert(Scancode::X);
        keys.insert(Scancode::Return);
        keys.insert(Scancode::Left);
        assert_eq!(keyboard_bits(&p, &keys), 1 | 8 | 64);
    }

    #[test]
    fn controllers_put_the_thumb_on_nes_b() {
        let p = controller_default("Pad");
        assert_eq!((p.a.as_str(), p.b.as_str()), ("b", "a"));
        let pressed: HashSet<String> = ["a".to_string(), "start".to_string()].into_iter().collect();
        assert_eq!(pad_bits(&p, &pressed, 16), 2 | 8 | 16);
    }

    #[test]
    fn a_saved_profile_beats_the_default() {
        let mut profiles = Profiles::default();
        profiles.set(Profiles::KEYBOARD.into(), Profile { name: "Keyboard".into(), a: "K".into(), b: "J".into(), select: "Tab".into(), start: "Space".into() });
        let mut input = Input::new(profiles);
        input.key(Scancode::K, true);
        input.key(Scancode::X, true);
        assert_eq!(input.buttons().0, Buttons(1));
        assert!(input.is_start(None, "Space"));
        assert!(!input.is_start(None, "Return"));
        input.clear();
        assert_eq!(input.buttons().0, Buttons(0));
    }

    #[test]
    fn keyboard_time_control_uses_comma_and_period_with_shift() {
        let mut input = Input::new(Profiles::default());
        assert_eq!(input.time_speed(), 0);
        input.key(Scancode::Period, true);
        assert_eq!(input.time_speed(), TIME_SLOW);
        input.key(Scancode::LShift, true);
        assert_eq!(input.time_speed(), TIME_FAST);
        input.key(Scancode::Period, false);
        input.key(Scancode::Comma, true);
        assert_eq!(input.time_speed(), -TIME_FAST);
        assert_eq!(input.jump_back(Scancode::Backspace, true), Some(15));
        assert_eq!(input.jump_back(Scancode::Backspace, false), Some(5));
        assert_eq!(input.jump_back(Scancode::A, false), None);
    }

    #[test]
    fn the_wizard_takes_four_distinct_buttons_from_one_device() {
        let mut w = Wizard::new();
        assert_eq!(w.step(), 0);
        assert_eq!(w.press("pad1", "Pad", "b"), WizardEvent::Advanced);
        assert_eq!(w.device_name(), Some("Pad"));
        assert_eq!(w.press("pad2", "Other", "a"), WizardEvent::Rejected);
        assert_eq!(w.press("pad1", "Pad", "b"), WizardEvent::Rejected);
        assert_eq!(w.press("pad1", "Pad", "a"), WizardEvent::Advanced);
        assert_eq!(w.press("pad1", "Pad", "back"), WizardEvent::Advanced);
        assert_eq!(w.step(), 3);
        match w.press("pad1", "Pad", "start") {
            WizardEvent::Done(key, profile) => {
                assert_eq!(key, "pad1");
                assert_eq!(profile, Profile { name: "Pad".into(), a: "b".into(), b: "a".into(), select: "back".into(), start: "start".into() });
            }
            other => panic!("{other:?}"),
        }
    }
}
```

The `Pad` tests need a real `GameController`, which needs SDL; they are covered by the pure helpers and by manual testing in Task 15.

- [ ] **Step 2: Implement input.rs**

```rust
//! Who is holding what. The keyboard and the first controller share port 1,
//! the second controller is port 2, and a saved profile beats the built-in
//! layout, which is the whole point of the mapping wizard.

use crate::settings::{Profile, Profiles};
use nes_core::Buttons;
use sdl2::controller::{Axis, Button, GameController};
use sdl2::keyboard::Scancode;
use std::collections::HashSet;

pub const TIME_SLOW: i32 = 2;
pub const TIME_FAST: i32 = 6;
const STICK_DEAD_ZONE: i16 = 16384;
const TRIGGER_ON: i16 = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NesButton {
    A,
    B,
    Select,
    Start,
}

impl NesButton {
    pub const ORDER: [NesButton; 4] = [NesButton::A, NesButton::B, NesButton::Select, NesButton::Start];
    pub fn bit(self) -> u8 {
        match self {
            NesButton::A => 1,
            NesButton::B => 2,
            NesButton::Select => 4,
            NesButton::Start => 8,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            NesButton::A => "A",
            NesButton::B => "B",
            NesButton::Select => "Select",
            NesButton::Start => "Start",
        }
    }
}

pub fn keyboard_default() -> Profile {
    Profile { name: "Keyboard".into(), a: "X".into(), b: "Z".into(), select: "Right Shift".into(), start: "Return".into() }
}

/// SDL names the south face button "a" and the east one "b". NES A is the
/// right-hand button and B the left, and a thumb rests on the south button,
/// so it drives NES B. The other way round makes every game feel backwards.
pub fn controller_default(name: &str) -> Profile {
    Profile { name: name.into(), a: "b".into(), b: "a".into(), select: "back".into(), start: "start".into() }
}

fn physical_bits(profile: &Profile, held: impl Fn(&str) -> bool) -> u8 {
    let mut bits = 0;
    for (button, name) in [(NesButton::A, &profile.a), (NesButton::B, &profile.b), (NesButton::Select, &profile.select), (NesButton::Start, &profile.start)] {
        if held(name) {
            bits |= button.bit();
        }
    }
    bits
}

pub fn keyboard_bits(profile: &Profile, keys: &HashSet<Scancode>) -> u8 {
    let mut bits = physical_bits(profile, |name| Scancode::from_name(name).is_some_and(|s| keys.contains(&s)));
    for (key, bit) in [(Scancode::Up, 16), (Scancode::Down, 32), (Scancode::Left, 64), (Scancode::Right, 128)] {
        if keys.contains(&key) {
            bits |= bit;
        }
    }
    bits
}

pub fn pad_bits(profile: &Profile, pressed: &HashSet<String>, axes: u8) -> u8 {
    physical_bits(profile, |name| pressed.contains(name)) | axes
}

pub struct Pad {
    pub instance: u32,
    pub guid: String,
    pub name: String,
    pub port: usize,
    _controller: GameController,
    pressed: HashSet<String>,
    dpad: u8,
    stick: u8,
    time: i32,
}

pub struct Input {
    profiles: Profiles,
    keys: HashSet<Scancode>,
    pads: Vec<Pad>,
    dirty: bool,
}

impl Input {
    pub fn new(profiles: Profiles) -> Input {
        Input { profiles, keys: HashSet::new(), pads: Vec::new(), dirty: false }
    }
    pub fn profiles(&self) -> &Profiles {
        &self.profiles
    }
    pub fn set_profile(&mut self, key: String, profile: Profile) {
        self.profiles.set(key, profile);
        self.dirty = true;
    }
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    pub fn key(&mut self, scancode: Scancode, pressed: bool) {
        if pressed {
            self.keys.insert(scancode);
        } else {
            self.keys.remove(&scancode);
        }
    }

    pub fn clear(&mut self) {
        self.keys.clear();
        for pad in &mut self.pads {
            pad.pressed.clear();
            pad.dpad = 0;
            pad.stick = 0;
            pad.time = 0;
        }
    }

    pub fn pad_added(&mut self, controller: GameController, guid: String) -> Option<usize> {
        let instance = controller.instance_id();
        if self.pads.iter().any(|p| p.instance == instance) {
            return None;
        }
        let port = (0..2).find(|port| !self.pads.iter().any(|p| p.port == *port))?;
        let name = controller.name();
        self.pads.push(Pad { instance, guid, name, port, _controller: controller, pressed: HashSet::new(), dpad: 0, stick: 0, time: 0 });
        Some(port)
    }

    pub fn pad_removed(&mut self, instance: u32) -> Option<String> {
        let at = self.pads.iter().position(|p| p.instance == instance)?;
        Some(self.pads.remove(at).name)
    }

    pub fn pad(&self, instance: u32) -> Option<&Pad> {
        self.pads.iter().find(|p| p.instance == instance)
    }

    pub fn pad_button(&mut self, instance: u32, button: Button, pressed: bool) {
        let Some(pad) = self.pads.iter_mut().find(|p| p.instance == instance) else {
            return;
        };
        let name = button.string();
        let dpad = match button {
            Button::DPadUp => 16,
            Button::DPadDown => 32,
            Button::DPadLeft => 64,
            Button::DPadRight => 128,
            _ => 0,
        };
        if dpad != 0 {
            if pressed { pad.dpad |= dpad } else { pad.dpad &= !dpad }
            return;
        }
        let time = match button {
            Button::RightShoulder => TIME_SLOW,
            Button::LeftShoulder => -TIME_SLOW,
            _ => 0,
        };
        if time != 0 {
            pad.time = if pressed { time } else { 0 };
            return;
        }
        if pressed { pad.pressed.insert(name); } else { pad.pressed.remove(&name); }
    }

    pub fn pad_axis(&mut self, instance: u32, axis: Axis, value: i16) {
        let Some(pad) = self.pads.iter_mut().find(|p| p.instance == instance) else {
            return;
        };
        match axis {
            Axis::LeftX => {
                pad.stick &= !(64 | 128);
                if value < -STICK_DEAD_ZONE { pad.stick |= 64 } else if value > STICK_DEAD_ZONE { pad.stick |= 128 }
            }
            Axis::LeftY => {
                pad.stick &= !(16 | 32);
                if value < -STICK_DEAD_ZONE { pad.stick |= 16 } else if value > STICK_DEAD_ZONE { pad.stick |= 32 }
            }
            Axis::TriggerRight => pad.time = if value > TRIGGER_ON { TIME_FAST } else { 0 },
            Axis::TriggerLeft => pad.time = if value > TRIGGER_ON { -TIME_FAST } else { 0 },
            _ => {}
        }
    }

    fn keyboard_profile(&self) -> Profile {
        self.profiles.get(Profiles::KEYBOARD).cloned().unwrap_or_else(keyboard_default)
    }

    fn pad_profile(&self, pad: &Pad) -> Profile {
        self.profiles.get(&pad.guid).cloned().unwrap_or_else(|| controller_default(&pad.name))
    }

    pub fn buttons(&self) -> (Buttons, Buttons) {
        let mut ports = [keyboard_bits(&self.keyboard_profile(), &self.keys), 0];
        for pad in &self.pads {
            ports[pad.port] |= pad_bits(&self.pad_profile(pad), &pad.pressed, pad.dpad | pad.stick);
        }
        (Buttons(ports[0]), Buttons(ports[1]))
    }

    /// Shoulders and triggers, or `,` and `.` with Shift, held to apply.
    pub fn time_speed(&self) -> i32 {
        if let Some(pad) = self.pads.iter().find(|p| p.time != 0) {
            return pad.time;
        }
        let shift = self.keys.contains(&Scancode::LShift) || self.keys.contains(&Scancode::RShift);
        let speed = if shift { TIME_FAST } else { TIME_SLOW };
        if self.keys.contains(&Scancode::Period) {
            speed
        } else if self.keys.contains(&Scancode::Comma) {
            -speed
        } else {
            0
        }
    }

    pub fn jump_back(&mut self, scancode: Scancode, shift: bool) -> Option<u32> {
        (scancode == Scancode::Backspace).then_some(if shift { 15 } else { 5 })
    }

    /// Whether a press is Start, through the saved profile, so a remapped
    /// Start still resumes a paused game.
    pub fn is_start(&self, instance: Option<u32>, physical: &str) -> bool {
        let profile = match instance.and_then(|i| self.pad(i)) {
            Some(pad) => self.pad_profile(pad),
            None => self.keyboard_profile(),
        };
        profile.start == physical
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum WizardEvent {
    Advanced,
    Rejected,
    Done(String, Profile),
}

/// Four steps: A, B, Select, Start, all from the device that pressed first,
/// each a button not already used.
#[derive(Default)]
pub struct Wizard {
    device: Option<(String, String)>,
    captured: Vec<(NesButton, String)>,
}

impl Wizard {
    pub fn new() -> Wizard {
        Wizard::default()
    }
    pub fn step(&self) -> usize {
        self.captured.len()
    }
    pub fn device_name(&self) -> Option<&str> {
        self.device.as_ref().map(|d| d.1.as_str())
    }
    pub fn press(&mut self, device_key: &str, device_name: &str, physical: &str) -> WizardEvent {
        match &self.device {
            Some((key, _)) if key != device_key => return WizardEvent::Rejected,
            None => self.device = Some((device_key.to_string(), device_name.to_string())),
            _ => {}
        }
        if self.captured.iter().any(|(_, p)| p == physical) {
            return WizardEvent::Rejected;
        }
        let button = NesButton::ORDER[self.captured.len()];
        self.captured.push((button, physical.to_string()));
        if self.captured.len() < 4 {
            return WizardEvent::Advanced;
        }
        let find = |b: NesButton| self.captured.iter().find(|(x, _)| *x == b).map(|(_, p)| p.clone()).unwrap_or_default();
        WizardEvent::Done(
            device_key.to_string(),
            Profile { name: device_name.to_string(), a: find(NesButton::A), b: find(NesButton::B), select: find(NesButton::Select), start: find(NesButton::Start) },
        )
    }
}
```

Add `mod input;` to `main.rs`.

- [ ] **Step 3: Run tests and clippy**

Run: `cargo test -p nes-desktop input && cargo clippy -p nes-desktop --all-targets --no-deps -- -D warnings`
Expected: 6 pass, clean (`#[allow(dead_code)]` on `mod input;` until Task 9).

- [ ] **Step 4: Commit**

```bash
git add desktop/src/input.rs desktop/src/main.rs
git commit -m "Add desktop input: keyboard and controller ports, profiles, time control, wizard"
```

---

### Task 8: The OpenGL picture pipeline and the ported looks

**Files:**
- Create: `desktop/src/video.rs`, `desktop/src/shaders/quad.vert`, `desktop/src/shaders/smooth.frag`, `desktop/src/shaders/composite.frag`, `desktop/src/shaders/present.frag`
- Modify: `scripts/check-shaders.py`, `desktop/src/main.rs` (add `mod video;`)

**Interfaces:**
- Consumes: `picture::{layout, visible_height, trim_fraction, Aspect, Look, Source, WIDTH, HEIGHT}`, `glow`
- Produces: `struct Video` with `new(gl: Arc<glow::Context>) -> Result<Video, String>`, `upload(&mut self, rgba: &[u8])`, `draw(&mut self, area: Viewport, look: Look, aspect: Aspect, trim: bool)` (into the window framebuffer; `area` is the region in device pixels with a bottom-left origin that the picture must fit inside, so the shell can keep the picture below a title bar and above the time row; the letterboxed viewport is `layout(area.width, area.height, ..)` offset by `area.x, area.y`; caller clears the screen first), `preview(&mut self, rgba: &[u8], look: Look, aspect: Aspect, trim: bool, width: i32, height: i32) -> Result<Vec<u8>, String>` (RGBA top-down, `width*height*4`), `destroy(&mut self)`
- `const SMOOTH_STEPS: usize = 2`, `const SUBSAMPLES: i32 = 4`

- [ ] **Step 1: Write the shaders**

`desktop/src/shaders/quad.vert`:

```glsl
#version 330 core
layout(location = 0) in vec2 p;
layout(location = 1) in vec2 uv;
out vec2 tex;
void main() { gl_Position = vec4(p, 0.0, 1.0); tex = uv; }
```

`desktop/src/shaders/smooth.frag`: the `SMOOTH_FRAGMENT` source from `ScreenRenderer.kt` with the first three lines replaced by `#version 330 core` (drop both `precision` lines), keeping every other line and comment. Write the comments as GLSL `//` comments above the shader body.

`desktop/src/shaders/composite.frag`: the `NTSC_FRAGMENT` source, same substitution.

`desktop/src/shaders/present.frag`: the `FRAGMENT` source, with `#version 330 core` and no `precision mediump float;`. Everything else identical, including `kind`, `cols`, `rows`, `window`, and the Cartoon `textureLod(screen, toTexture(p), 4.0)`.

- [ ] **Step 2: Validate the shaders with the existing checker**

Extend `scripts/check-shaders.py`: add `DESKTOP = ROOT / "desktop/src/shaders"` and in `shaders()` after the Kotlin scan append `(path.stem + "-desktop", path.read_text(), path.suffix[1:])` for every `*.vert` and `*.frag` in `DESKTOP`, sorted. Update the docstring's first line to "Compile-check the GLSL in ScreenRenderer.kt and desktop/src/shaders without a device."

Run: `python3 scripts/check-shaders.py` Expected: `ok:` for each of the four desktop shaders, or `skipped` if no validator is installed (install with `brew install glslang` on macOS to check locally).

- [ ] **Step 3: Implement video.rs**

```rust
//! The picture on the GPU: the framebuffer as a texture, the two off-screen
//! preparation passes, the present shader with one branch per look, and the
//! preview readback the settings panel shows. Ported from ScreenRenderer.kt.

use crate::picture::{layout, trim_fraction, visible_height, Aspect, Look, Source, Viewport, HEIGHT, WIDTH};
use glow::HasContext;
use std::sync::Arc;

pub const SMOOTH_STEPS: usize = 2;
pub const SUBSAMPLES: i32 = 4;

const QUAD_VERT: &str = include_str!("shaders/quad.vert");
const SMOOTH_FRAG: &str = include_str!("shaders/smooth.frag");
const COMPOSITE_FRAG: &str = include_str!("shaders/composite.frag");
const PRESENT_FRAG: &str = include_str!("shaders/present.frag");

struct Program {
    id: glow::Program,
}

pub struct Video {
    gl: Arc<glow::Context>,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    frame: glow::Texture,
    present: Program,
    smooth: Program,
    composite: Program,
    smooth_textures: [glow::Texture; SMOOTH_STEPS],
    smooth_buffers: [glow::Framebuffer; SMOOTH_STEPS],
    ntsc_texture: glow::Texture,
    ntsc_buffer: glow::Framebuffer,
    preview: Option<(glow::Framebuffer, glow::Texture, i32, i32)>,
    frames: u32,
}

unsafe fn compile(gl: &glow::Context, kind: u32, source: &str) -> Result<glow::Shader, String> {
    let shader = gl.create_shader(kind)?;
    gl.shader_source(shader, source);
    gl.compile_shader(shader);
    if !gl.get_shader_compile_status(shader) {
        return Err(format!("shader: {}", gl.get_shader_info_log(shader)));
    }
    Ok(shader)
}

unsafe fn link(gl: &glow::Context, fragment: &str) -> Result<Program, String> {
    let v = compile(gl, glow::VERTEX_SHADER, QUAD_VERT)?;
    let f = compile(gl, glow::FRAGMENT_SHADER, fragment)?;
    let id = gl.create_program()?;
    gl.attach_shader(id, v);
    gl.attach_shader(id, f);
    gl.link_program(id);
    if !gl.get_program_link_status(id) {
        return Err(format!("program: {}", gl.get_program_info_log(id)));
    }
    gl.delete_shader(v);
    gl.delete_shader(f);
    Ok(Program { id })
}

unsafe fn texture(gl: &glow::Context, width: i32, height: i32, linear: bool, mipmap: bool) -> Result<glow::Texture, String> {
    let t = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(t));
    let min = if mipmap { glow::LINEAR_MIPMAP_LINEAR } else if linear { glow::LINEAR } else { glow::NEAREST };
    let mag = if linear || mipmap { glow::LINEAR } else { glow::NEAREST };
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, min as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, mag as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, width, height, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
    if mipmap {
        gl.generate_mipmap(glow::TEXTURE_2D);
    }
    Ok(t)
}

unsafe fn framebuffer(gl: &glow::Context, t: glow::Texture) -> Result<glow::Framebuffer, String> {
    let f = gl.create_framebuffer()?;
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(f));
    gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(t), 0);
    let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
    gl.bind_framebuffer(glow::FRAMEBUFFER, None);
    if status != glow::FRAMEBUFFER_COMPLETE {
        return Err(format!("framebuffer incomplete: {status}"));
    }
    Ok(f)
}

impl Video {
    pub fn new(gl: Arc<glow::Context>) -> Result<Video, String> {
        unsafe {
            let vao = gl.create_vertex_array()?;
            let vbo = gl.create_buffer()?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_size(glow::ARRAY_BUFFER, 16 * 4, glow::DYNAMIC_DRAW);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 16, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, 16, 8);
            gl.bind_vertex_array(None);
            let frame = texture(&gl, WIDTH as i32, HEIGHT as i32, false, false)?;
            let mut smooth_textures = Vec::new();
            let mut smooth_buffers = Vec::new();
            for step in 0..SMOOTH_STEPS {
                let scale = 2 << step;
                let last = step == SMOOTH_STEPS - 1;
                let t = texture(&gl, WIDTH as i32 * scale, HEIGHT as i32 * scale, last, last)?;
                smooth_buffers.push(framebuffer(&gl, t)?);
                smooth_textures.push(t);
            }
            let ntsc_texture = texture(&gl, WIDTH as i32 * SUBSAMPLES, HEIGHT as i32, true, false)?;
            let ntsc_buffer = framebuffer(&gl, ntsc_texture)?;
            Ok(Video {
                present: link(&gl, PRESENT_FRAG)?,
                smooth: link(&gl, SMOOTH_FRAG)?,
                composite: link(&gl, COMPOSITE_FRAG)?,
                gl,
                vao,
                vbo,
                frame,
                smooth_textures: [smooth_textures[0], smooth_textures[1]],
                smooth_buffers: [smooth_buffers[0], smooth_buffers[1]],
                ntsc_texture,
                ntsc_buffer,
                preview: None,
                frames: 0,
            })
        }
    }

    pub fn upload(&mut self, rgba: &[u8]) {
        unsafe {
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.frame));
            self.gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
            self.gl.tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, WIDTH as i32, HEIGHT as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(rgba)));
        }
    }

    /// Texture rows run top-down while the quad runs bottom-up, so the top of
    /// the picture is v = trim and the bottom is 1 - trim.
    unsafe fn quad(&self, trim: bool) {
        let edge = trim_fraction(trim);
        let vertices: [f32; 16] = [-1.0, -1.0, 0.0, 1.0 - edge, 1.0, -1.0, 1.0, 1.0 - edge, -1.0, 1.0, 0.0, edge, 1.0, 1.0, 1.0, edge];
        self.gl.bind_vertex_array(Some(self.vao));
        self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
        self.gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, bytemuck_cast(&vertices));
        self.gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
    }

    unsafe fn smooth_pass(&mut self, bound: Option<glow::Framebuffer>, viewport: [i32; 4]) -> glow::Texture {
        let gl = &self.gl;
        gl.use_program(Some(self.smooth.id));
        gl.uniform_1_i32(gl.get_uniform_location(self.smooth.id, "src").as_ref(), 0);
        let mut source = self.frame;
        for step in 0..SMOOTH_STEPS {
            let scale = 1 << step;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.smooth_buffers[step]));
            gl.viewport(0, 0, WIDTH as i32 * scale * 2, HEIGHT as i32 * scale * 2);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(source));
            gl.uniform_2_i32(gl.get_uniform_location(self.smooth.id, "srcSize").as_ref(), WIDTH as i32 * scale, HEIGHT as i32 * scale);
            self.quad(false);
            source = self.smooth_textures[step];
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, bound);
        gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
        gl.bind_texture(glow::TEXTURE_2D, Some(source));
        gl.generate_mipmap(glow::TEXTURE_2D);
        source
    }

    unsafe fn composite_pass(&mut self, bound: Option<glow::Framebuffer>, viewport: [i32; 4]) -> glow::Texture {
        let gl = &self.gl;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.ntsc_buffer));
        gl.viewport(0, 0, WIDTH as i32 * SUBSAMPLES, HEIGHT as i32);
        gl.use_program(Some(self.composite.id));
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(self.frame));
        gl.uniform_1_i32(gl.get_uniform_location(self.composite.id, "src").as_ref(), 0);
        gl.uniform_2_i32(gl.get_uniform_location(self.composite.id, "srcSize").as_ref(), WIDTH as i32, HEIGHT as i32);
        gl.uniform_1_f32(gl.get_uniform_location(self.composite.id, "framePhase").as_ref(), (self.frames % 3) as f32 / 3.0);
        self.quad(false);
        gl.bind_framebuffer(glow::FRAMEBUFFER, bound);
        gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
        self.ntsc_texture
    }

    /// Draws the uploaded frame through `look` into the bound framebuffer,
    /// letterboxed by `layout`. `bound` is the framebuffer to return to after
    /// any preparation pass (None for the window).
    unsafe fn draw_into(&mut self, bound: Option<glow::Framebuffer>, area: Viewport, look: Look, aspect: Aspect, trim: bool) {
        let mut view = layout(area.width, area.height, aspect, trim);
        view.x += area.x;
        view.y += area.y;
        let viewport = [view.x, view.y, view.width, view.height];
        self.frames = self.frames.wrapping_add(1);
        let present = match look.source() {
            Source::Direct => self.frame,
            Source::Smoothed => self.smooth_pass(bound, viewport),
            Source::Composite => self.composite_pass(bound, viewport),
        };
        let gl = &self.gl;
        gl.viewport(view.x, view.y, view.width, view.height);
        gl.use_program(Some(self.present.id));
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(present));
        gl.uniform_1_i32(gl.get_uniform_location(self.present.id, "screen").as_ref(), 0);
        gl.uniform_1_i32(gl.get_uniform_location(self.present.id, "kind").as_ref(), look.id());
        gl.uniform_1_f32(gl.get_uniform_location(self.present.id, "cols").as_ref(), WIDTH as f32);
        gl.uniform_1_f32(gl.get_uniform_location(self.present.id, "rows").as_ref(), visible_height(trim) as f32);
        let edge = trim_fraction(trim);
        gl.uniform_2_f32(gl.get_uniform_location(self.present.id, "window").as_ref(), edge, 1.0 - 2.0 * edge);
        self.quad(trim);
    }

    pub fn draw(&mut self, area: Viewport, look: Look, aspect: Aspect, trim: bool) {
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
            self.gl.disable(glow::BLEND);
            self.draw_into(None, area, look, aspect, trim);
        }
    }

    /// One still through the real pipeline, read back top-down.
    pub fn preview(&mut self, rgba: &[u8], look: Look, aspect: Aspect, trim: bool, width: i32, height: i32) -> Result<Vec<u8>, String> {
        unsafe {
            if self.preview.is_none_or(|(_, _, w, h)| (w, h) != (width, height)) {
                if let Some((f, t, _, _)) = self.preview.take() {
                    self.gl.delete_framebuffer(f);
                    self.gl.delete_texture(t);
                }
                let t = texture(&self.gl, width, height, true, false)?;
                let f = framebuffer(&self.gl, t)?;
                self.preview = Some((f, t, width, height));
            }
            let (f, _, _, _) = self.preview.unwrap();
            self.upload(rgba);
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(f));
            self.gl.disable(glow::SCISSOR_TEST);
            self.gl.disable(glow::BLEND);
            self.gl.viewport(0, 0, width, height);
            self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
            self.draw_into(Some(f), Viewport { x: 0, y: 0, width, height }, look, aspect, trim);
            let mut out = vec![0u8; (width * height * 4) as usize];
            self.gl.read_pixels(0, 0, width, height, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut out)));
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            // GL reads bottom-up.
            let row = (width * 4) as usize;
            let mut flipped = vec![0u8; out.len()];
            for y in 0..height as usize {
                flipped[y * row..(y + 1) * row].copy_from_slice(&out[(height as usize - 1 - y) * row..(height as usize - y) * row]);
            }
            Ok(flipped)
        }
    }

    pub fn destroy(&mut self) {
        unsafe {
            self.gl.delete_program(self.present.id);
            self.gl.delete_program(self.smooth.id);
            self.gl.delete_program(self.composite.id);
            self.gl.delete_texture(self.frame);
            for t in self.smooth_textures { self.gl.delete_texture(t); }
            for f in self.smooth_buffers { self.gl.delete_framebuffer(f); }
            self.gl.delete_texture(self.ntsc_texture);
            self.gl.delete_framebuffer(self.ntsc_buffer);
            if let Some((f, t, _, _)) = self.preview.take() {
                self.gl.delete_framebuffer(f);
                self.gl.delete_texture(t);
            }
            self.gl.delete_buffer(self.vbo);
            self.gl.delete_vertex_array(self.vao);
        }
    }
}

fn bytemuck_cast(vertices: &[f32; 16]) -> &[u8] {
    // f32 to native-endian bytes for the VBO; the same memory, reinterpreted.
    unsafe { std::slice::from_raw_parts(vertices.as_ptr() as *const u8, std::mem::size_of_val(vertices)) }
}
```

Check the exact glow 0.17 signatures in `~/.cargo/registry/src/*/glow-0.17.0/src/native.rs` for `tex_image_2d`, `tex_sub_image_2d`, `read_pixels` (their `PixelUnpackData`/`PixelPackData` enum shapes) and `is_none_or` (Rust 1.82+; use `map_or(true, ...)` if the toolchain is older). Add a smoke test that only checks the shader sources embed:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn the_shaders_are_desktop_glsl_with_the_android_constants() {
        for s in [super::SMOOTH_FRAG, super::COMPOSITE_FRAG, super::PRESENT_FRAG, super::QUAD_VERT] {
            assert!(s.starts_with("#version 330 core"));
            assert!(!s.contains("precision "));
        }
        assert!(super::COMPOSITE_FRAG.contains("const float SUBS = 4.0;"));
        assert!(super::PRESENT_FRAG.contains("textureLod(screen, toTexture(p), 4.0)"));
        assert!(super::PRESENT_FRAG.contains("kind == 10") || !super::PRESENT_FRAG.contains("kind == 9"), "Smooth and Composite take no branch");
    }
}
```

Add `mod video;` to `main.rs` (with `#[allow(dead_code)]` until Task 9).

- [ ] **Step 4: Build, test, clippy**

Run: `cargo test -p nes-desktop video && cargo clippy -p nes-desktop --all-targets --no-deps -- -D warnings && python3 scripts/check-shaders.py`
Expected: pass; clippy may ask for `# Safety` docs on `unsafe fn`: make the helpers private and add `// SAFETY:` comments, or `#[allow(clippy::missing_safety_doc)]` on the module.

- [ ] **Step 5: Commit**

```bash
git add desktop/src/video.rs desktop/src/shaders scripts/check-shaders.py desktop/src/main.rs
git commit -m "Port the Android picture pipeline and looks to OpenGL 3.3 on desktop"
```

---

### Task 9: The window: SDL, OpenGL, the egui bridge and a playable loop

**Files:**
- Create: `desktop/src/bridge.rs`, `desktop/src/ui/mod.rs`
- Rewrite: `desktop/src/shell.rs`
- Modify: `desktop/src/main.rs` (add `mod bridge; mod ui;`, remove `#[allow(dead_code)]` lines that are no longer needed)

**Interfaces:**
- Consumes: everything from Tasks 1 to 8; `egui`, `egui_glow`, `glow`, `sdl2`
- Produces (`bridge.rs`):
  - `struct Bridge { pub ctx: egui::Context, painter: egui_glow::Painter, events: Vec<egui::Event>, modifiers: egui::Modifiers, start: Instant }`
  - `Bridge::new(gl: Arc<glow::Context>) -> Result<Bridge, String>`
  - `Bridge::handle(&mut self, event: &sdl2::event::Event)` (translates pointer, wheel, key, text, focus events; keeps `modifiers` current)
  - `Bridge::modifiers(&self) -> egui::Modifiers`
  - `Bridge::begin(&mut self, window: &sdl2::video::Window)` (builds `RawInput` with `screen_rect` in points, `pixels_per_point = drawable_w / window_w`, `time`, `modifiers`, drained events; calls `ctx.begin_pass`)
  - `Bridge::end(&mut self, window: &sdl2::video::Window) -> egui::PlatformOutput` (calls `ctx.end_pass`, tessellates, `painter.paint_and_update_textures`)
  - `Bridge::destroy(&mut self)`
  - `pub fn key_name(scancode: sdl2::keyboard::Scancode) -> String` (`scancode.name()`)
- Produces (`ui/mod.rs`): the shared UI state types used by every later task:

```rust
pub mod theme;      // Task 10
pub mod widgets;    // Task 10
pub mod shelf;      // Task 11
pub mod time;       // Task 12
pub mod play;       // Task 12
pub mod panels;     // Task 13
pub mod settings;   // Task 14

use crate::library::Game;
use crate::picture::{Aspect, Look, PaletteChoice};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel { None, Pause, Slots, Settings, Mapping, Problems }

#[derive(Clone, Debug, PartialEq)]
pub enum Dialog {
    Message(String),
    ConfirmReset,
    ConfirmReplace(u8),
    ConfirmDelete(Game),
}

/// What a frame of UI asked for. Drawn code pushes these; the shell applies
/// them after the frame, so the panels never touch the engine or the disk.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Import,
    OpenGame(Game),
    ChooseArt(Game),
    ClearArt(Game),
    SetArchived(Game, bool),
    DeleteRequested(Game),
    DeleteConfirmed(Game),
    ShowArchive(bool),
    ToggleShelfList,
    OpenSettings,
    CloseSettings,
    OpenProblems,
    ClosePanel,
    Pause,
    Resume,
    OpenSlots,
    SaveRequested(u8),
    SaveConfirmed(u8),
    Load(u8),
    Screenshot,
    ToggleFullscreen,
    ResetRequested,
    ResetConfirmed,
    BackToShelf,
    SetLook(Look),
    SetAspect(Aspect),
    SetPalette(PaletteChoice),
    SetTrim(bool),
    ImportPalette,
    StartWizard,
    CancelWizard,
    Scrub(f32),
    ScrubReleased,
    JumpBack(u32),
    CloseDialog,
}
```

  For this task only `Pause`, `Resume`, `ToggleFullscreen`, `Screenshot`, `SaveConfirmed`, `Load`, `JumpBack`, `Scrub`, `ScrubReleased` and `BackToShelf` are handled; the rest are wired as their panels arrive (Tasks 11 to 14). An unhandled action is a no-op with `eprintln!("unhandled {action:?}")` so nothing silently disappears.

- Produces (`shell.rs`): `pub struct App` (fields below) and `pub fn run(options: Options) -> Result<(), String>`.

```rust
pub struct App {
    pub data_dir: PathBuf,
    pub library: Library,
    pub settings: Settings,
    pub settings_dirty: bool,
    pub input: Input,
    pub session: Option<Session>,
    pub panel: Panel,
    pub panel_before: Panel,             // where ClosePanel returns from the problem log
    pub dialog: Option<Dialog>,
    pub show_archive: bool,
    pub notice: Option<(String, Instant)>,
    pub message: Option<String>,          // the plain sentence shown until dismissed
    pub busy: bool,
    pub fullscreen: bool,
    pub chrome_until: Instant,
    pub wizard: Option<Wizard>,
    pub scrub_fraction: f32,              // where the handle is, -1..1
    pub rewind_depth: usize,
    pub audio_ms: f32,
    pub covers: HashMap<String, (i64, egui::TextureHandle)>,   // by game id, keyed on mtime
    pub thumbs: HashMap<u8, (i64, egui::TextureHandle)>,       // slot thumbnails for the open game
    pub preview: Option<egui::TextureHandle>,
    pub preview_dirty: bool,
    pub actions: Vec<Action>,
    pub quit: bool,
}
```

  Helper methods on `App` used by later tasks: `report(&mut self, label: &str, detail: &str)` (logs through the library and sets `message`), `notice(&mut self, text: &str)` (2.2 s HUD pill), `cover_texture(&mut self, ctx: &egui::Context, game: &Game) -> Option<egui::TextureHandle>` (loads `library.cover` through `files::read_png`, cached by mtime), `slot_texture(&mut self, ctx, slot: &SaveSlot) -> Option<TextureHandle>`, `sample_source(&self) -> Vec<u8>` (paused frame, newest autosave thumbnail, or `picture::sample_frame`).

- [ ] **Step 1: Write bridge.rs**

```rust
//! SDL events in, egui paint out. Small on purpose: the parts of a windowing
//! backend that a game with a handful of panels needs, and nothing else.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use sdl2::event::{Event as SdlEvent, WindowEvent};
use sdl2::keyboard::{Keycode, Mod, Scancode};
use sdl2::mouse::MouseButton;
use sdl2::video::Window;
use std::{sync::Arc, time::Instant};

pub struct Bridge {
    pub ctx: egui::Context,
    painter: egui_glow::Painter,
    events: Vec<Event>,
    modifiers: Modifiers,
    start: Instant,
}

pub fn key_name(scancode: Scancode) -> String {
    scancode.name().to_string()
}

fn modifiers(keymod: Mod) -> Modifiers {
    let mac = cfg!(target_os = "macos");
    let cmd = keymod.intersects(Mod::LGUIMOD | Mod::RGUIMOD);
    let ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
    Modifiers {
        alt: keymod.intersects(Mod::LALTMOD | Mod::RALTMOD),
        ctrl,
        shift: keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD),
        mac_cmd: mac && cmd,
        command: if mac { cmd } else { ctrl },
    }
}

fn key(keycode: Keycode) -> Option<Key> {
    Key::from_name(&keycode.name())
}

fn button(b: MouseButton) -> Option<PointerButton> {
    match b {
        MouseButton::Left => Some(PointerButton::Primary),
        MouseButton::Right => Some(PointerButton::Secondary),
        MouseButton::Middle => Some(PointerButton::Middle),
        _ => None,
    }
}

impl Bridge {
    pub fn new(gl: Arc<glow::Context>) -> Result<Bridge, String> {
        let painter = egui_glow::Painter::new(gl, "", None, false).map_err(|e| e.to_string())?;
        Ok(Bridge { ctx: egui::Context::default(), painter, events: Vec::new(), modifiers: Modifiers::default(), start: Instant::now() })
    }

    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    pub fn handle(&mut self, event: &SdlEvent) {
        match event {
            SdlEvent::MouseMotion { x, y, .. } => self.events.push(Event::PointerMoved(Pos2::new(*x as f32, *y as f32))),
            SdlEvent::MouseButtonDown { mouse_btn, x, y, .. } | SdlEvent::MouseButtonUp { mouse_btn, x, y, .. } => {
                if let Some(button) = button(*mouse_btn) {
                    let pressed = matches!(event, SdlEvent::MouseButtonDown { .. });
                    self.events.push(Event::PointerButton { pos: Pos2::new(*x as f32, *y as f32), button, pressed, modifiers: self.modifiers });
                }
            }
            SdlEvent::MouseWheel { precise_x, precise_y, .. } => self.events.push(Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: Vec2::new(*precise_x, *precise_y),
                phase: egui::TouchPhase::Move,
                modifiers: self.modifiers,
            }),
            SdlEvent::KeyDown { keycode: Some(keycode), keymod, repeat, .. } | SdlEvent::KeyUp { keycode: Some(keycode), keymod, repeat, .. } => {
                self.modifiers = modifiers(*keymod);
                if let Some(key) = key(*keycode) {
                    let pressed = matches!(event, SdlEvent::KeyDown { .. });
                    self.events.push(Event::Key { key, physical_key: None, pressed, repeat: *repeat, modifiers: self.modifiers });
                }
            }
            SdlEvent::TextInput { text, .. } => self.events.push(Event::Text(text.clone())),
            SdlEvent::Window { win_event: WindowEvent::FocusGained, .. } => self.events.push(Event::WindowFocused(true)),
            SdlEvent::Window { win_event: WindowEvent::FocusLost, .. } => {
                self.events.push(Event::WindowFocused(false));
                self.events.push(Event::PointerGone);
            }
            SdlEvent::Window { win_event: WindowEvent::Leave, .. } => self.events.push(Event::PointerGone),
            _ => {}
        }
    }

    pub fn pixels_per_point(window: &Window) -> f32 {
        let (w, _) = window.size();
        let (dw, _) = window.drawable_size();
        if w == 0 { 1.0 } else { dw as f32 / w as f32 }
    }

    pub fn begin(&mut self, window: &Window) {
        let (w, h) = window.size();
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w as f32, h as f32))),
            time: Some(self.start.elapsed().as_secs_f64()),
            modifiers: self.modifiers,
            events: std::mem::take(&mut self.events),
            ..Default::default()
        };
        self.ctx.set_pixels_per_point(Self::pixels_per_point(window));
        self.ctx.begin_pass(input);
    }

    pub fn end(&mut self, window: &Window) -> egui::PlatformOutput {
        let output = self.ctx.end_pass();
        let (dw, dh) = window.drawable_size();
        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        let mut textures = output.textures_delta;
        self.painter.paint_and_update_textures([dw, dh], output.pixels_per_point, &primitives, &mut textures);
        output.platform_output
    }

    pub fn destroy(&mut self) {
        self.painter.destroy();
    }
}
```

Check `RawInput` field names against `egui-0.36.2/src/data/input/raw_input.rs` (it has `screen_rect`, `time`, `modifiers`, `events`, `focused`, `viewports`; `max_texture_side` is set by the painter through `Painter::max_texture_side()`: add `max_texture_side: Some(self.painter.max_texture_side())` to the `RawInput` if the field exists).

- [ ] **Step 2: Write ui/mod.rs**

The listing above, exactly, with the submodule lines commented out until each task creates its file (uncomment `theme` and `widgets` in Task 10, and so on).

- [ ] **Step 3: Rewrite shell.rs**

```rust
//! The window, the GL context, the main loop, and the App the panels draw
//! from. One thread: events, UI, emulation, picture, panels, swap.

use crate::bridge::{key_name, Bridge};
use crate::input::{Input, Wizard, WizardEvent};
use crate::library::{Game, Library, SaveSlot, Slot};
use crate::picture::{self, Viewport};
use crate::session::Session;
use crate::settings::{Profiles, Settings};
use crate::ui::{Action, Dialog, Panel};
use crate::video::Video;
use crate::Options;
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Scancode;
use sdl2::video::{FullscreenType, GLProfile, SwapInterval};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const CHROME_IDLE: Duration = Duration::from_secs(5);
pub const NOTICE_TIME: Duration = Duration::from_millis(2200);
pub const TITLE_BAR: f32 = 52.0;
pub const TIME_ROW: f32 = 84.0;
pub const END_OF_TAPE: &str = "That's as far back as this goes.";

pub struct App { /* exactly the fields listed under Interfaces above, plus `pub panel_before: Panel` (where the problem log returns to) */ }

impl App {
    pub fn report(&mut self, label: &str, detail: &str) {
        self.library.log_problem(label, detail);
        self.message = Some(label.to_string());
    }
    pub fn notice(&mut self, text: &str) {
        self.notice = Some((text.to_string(), Instant::now()));
    }
    pub fn playing(&self) -> bool {
        self.session.is_some() && self.panel == Panel::None && self.dialog.is_none() && !self.busy && self.wizard.is_none()
    }
    fn texture(ctx: &egui::Context, name: &str, path: &std::path::Path) -> Option<egui::TextureHandle> {
        let (w, h, rgba) = crate::files::read_png(path).ok()?;
        let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
        Some(ctx.load_texture(name, image, egui::TextureOptions::LINEAR))
    }
    fn mtime(path: &std::path::Path) -> i64 {
        std::fs::metadata(path).ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as i64)
    }
    pub fn cover_texture(&mut self, ctx: &egui::Context, game: &Game) -> Option<egui::TextureHandle> {
        let path = self.library.cover(&game.id)?;
        let key = Self::mtime(&path);
        if let Some((k, t)) = self.covers.get(&game.id) {
            if *k == key {
                return Some(t.clone());
            }
        }
        let texture = Self::texture(ctx, &format!("cover-{}", game.id), &path)?;
        self.covers.insert(game.id.clone(), (key, texture.clone()));
        Some(texture)
    }
    pub fn slot_texture(&mut self, ctx: &egui::Context, slot: &SaveSlot) -> Option<egui::TextureHandle> {
        let key = slot.time?;
        if let Some((k, t)) = self.thumbs.get(&slot.number) {
            if *k == key {
                return Some(t.clone());
            }
        }
        let texture = Self::texture(ctx, &format!("slot-{}", slot.number), &slot.thumbnail)?;
        self.thumbs.insert(slot.number, (key, texture.clone()));
        Some(texture)
    }
    /// The paused frame, else the newest saved moment on the shelf, else the
    /// built pattern; always the pattern when a palette other than Standard is
    /// chosen, because a saved PNG holds finished colour and cannot be restained.
    pub fn sample_source(&self) -> Vec<u8> {
        let standard = self.settings.palette == picture::PaletteChoice::Standard;
        if let Some(session) = &self.session {
            if standard {
                return session.engine.frame().to_vec();
            }
        } else if standard {
            for game in self.library.games() {
                if let Ok((w, h, rgba)) = crate::files::read_png(&self.library.thumbnail_path(&game.id, Slot::Auto)) {
                    if (w, h) == (256, 240) {
                        return rgba;
                    }
                }
            }
        }
        picture::sample_frame(&self.settings.preview_colours(&self.data_dir))
    }
}

pub fn run(options: Options) -> Result<(), String> {
    std::fs::create_dir_all(&options.data_dir).map_err(|e| format!("{}: {e}", options.data_dir.display()))?;
    let library = Library::open(&options.data_dir)?;
    let settings = Settings::load(&options.data_dir);
    let sdl = sdl2::init()?;
    let video_subsystem = sdl.video()?;
    let controllers = sdl.game_controller()?;
    let joysticks = sdl.joystick()?;
    {
        let attr = video_subsystem.gl_attr();
        attr.set_context_profile(GLProfile::Core);
        attr.set_context_version(3, 3);
        attr.set_double_buffer(true);
    }
    let mut window = video_subsystem
        .window("Emulia", 1024, 768)
        .position_centered()
        .resizable()
        .allow_highdpi()
        .opengl()
        .build()
        .map_err(|e| e.to_string())?;
    let gl_context = window.gl_create_context()?;
    window.gl_make_current(&gl_context)?;
    let gl = Arc::new(unsafe { glow::Context::from_loader_function(|s| video_subsystem.gl_get_proc_address(s) as *const _) });
    if video_subsystem.gl_set_swap_interval(SwapInterval::VSync).is_err() {
        eprintln!("vsync unavailable; pacing by sleep");
    }
    let mut video = Video::new(gl.clone()).map_err(|e| format!("OpenGL 3.3 is needed for the picture ({e}). Driver: {}", video_subsystem.current_video_driver()))?;
    let mut bridge = Bridge::new(gl)?;
    crate::ui::theme::apply(&bridge.ctx);
    let mut events = sdl.event_pump()?;
    let now = Instant::now();
    let mut app = App {
        data_dir: options.data_dir.clone(),
        library,
        fullscreen: settings.fullscreen,
        settings,
        settings_dirty: false,
        input: Input::new(Profiles::load(&options.data_dir)),
        session: None,
        panel: Panel::None,
        panel_before: Panel::None,
        dialog: None,
        show_archive: false,
        notice: None,
        message: None,
        busy: false,
        chrome_until: now + CHROME_IDLE,
        wizard: None,
        scrub_fraction: 0.0,
        rewind_depth: 0,
        audio_ms: 0.0,
        covers: HashMap::new(),
        thumbs: HashMap::new(),
        preview: None,
        preview_dirty: true,
        actions: Vec::new(),
        quit: false,
    };
    if app.fullscreen {
        let _ = window.set_fullscreen(FullscreenType::Desktop);
    }
    if let Some(rom) = &options.rom {
        match crate::import(&app.library, rom) {
            Ok(game) => app.actions.push(Action::OpenGame(game)),
            Err(e) => app.report("That game file didn't work.", &e),
        }
    }
    let mut audio_sdl = Some(sdl.clone());
    if options.mute {
        audio_sdl = None;
    }
    let mut last_frame_uploaded = false;
    while !app.quit {
        // 1. Events.
        for event in events.poll_iter() {
            bridge.handle(&event);
            match &event {
                Event::Quit { .. } => app.quit = true,
                Event::Window { win_event: WindowEvent::FocusLost, .. } => {
                    if app.session.is_some() && app.panel == Panel::None && app.wizard.is_none() {
                        app.actions.push(Action::Pause);
                    }
                }
                Event::DropFile { filename, .. } => app.actions.push(dropped(&app, PathBuf::from(filename))),
                Event::MouseMotion { .. } | Event::MouseButtonDown { .. } => app.chrome_until = Instant::now() + CHROME_IDLE,
                Event::KeyDown { scancode: Some(scancode), keymod, repeat: false, .. } => {
                    app.chrome_until = Instant::now() + CHROME_IDLE;
                    let shift = keymod.intersects(sdl2::keyboard::Mod::LSHIFTMOD | sdl2::keyboard::Mod::RSHIFTMOD);
                    if let Some(wizard) = &mut app.wizard {
                        match wizard.press(Profiles::KEYBOARD, "Keyboard", &key_name(*scancode)) {
                            WizardEvent::Done(key, profile) => finish_wizard(&mut app, key, profile),
                            _ => {}
                        }
                        continue;
                    }
                    if bridge.ctx.wants_keyboard_input() {
                        continue;
                    }
                    match scancode {
                        Scancode::Escape => app.actions.push(escape(&app)),
                        Scancode::Space => app.actions.push(if app.panel == Panel::Pause { Action::Resume } else if app.session.is_some() { Action::Pause } else { Action::CloseDialog }),
                        Scancode::F11 => app.actions.push(Action::ToggleFullscreen),
                        Scancode::F5 if app.playing() => app.actions.push(Action::SaveConfirmed(0)),
                        Scancode::F8 if app.playing() => app.actions.push(Action::Load(0)),
                        _ => {
                            if let Some(seconds) = app.input.jump_back(*scancode, shift) {
                                if app.playing() {
                                    app.actions.push(Action::JumpBack(seconds));
                                }
                            } else if app.panel == Panel::Pause && app.input.is_start(None, &key_name(*scancode)) {
                                app.actions.push(Action::Resume);
                            } else {
                                app.input.key(*scancode, true);
                            }
                        }
                    }
                }
                Event::KeyUp { scancode: Some(scancode), .. } => app.input.key(*scancode, false),
                Event::ControllerDeviceAdded { which, .. } => {
                    if let (Ok(controller), Ok(guid)) = (controllers.open(*which), joysticks.device_guid(*which)) {
                        app.input.pad_added(controller, guid.string());
                    }
                }
                Event::ControllerDeviceRemoved { which, .. } => {
                    if app.input.pad_removed(*which).is_some() && app.session.is_some() {
                        app.actions.push(Action::Pause);
                        app.message = Some("Controller disconnected. Your game is paused.".into());
                    }
                }
                Event::ControllerButtonDown { which, button, .. } => {
                    app.chrome_until = Instant::now() + CHROME_IDLE;
                    let physical = button.string();
                    if let Some(wizard) = &mut app.wizard {
                        if let Some(pad) = app.input.pad(*which) {
                            let (guid, name) = (pad.guid.clone(), pad.name.clone());
                            if let WizardEvent::Done(key, profile) = wizard.press(&guid, &name, &physical) {
                                finish_wizard(&mut app, key, profile);
                            }
                        }
                        continue;
                    }
                    if app.panel == Panel::Pause && app.input.is_start(Some(*which), &physical) {
                        app.actions.push(Action::Resume);
                    }
                    app.input.pad_button(*which, *button, true);
                }
                Event::ControllerButtonUp { which, button, .. } => app.input.pad_button(*which, *button, false),
                Event::ControllerAxisMotion { which, axis, value, .. } => app.input.pad_axis(*which, *axis, *value),
                _ => {}
            }
        }
        // 2. UI.
        bridge.begin(&window);
        draw(&bridge.ctx, &mut app, &mut video);
        let (_platform, primitives, mut textures) = bridge.finish();
        // 3. Actions.
        let actions = std::mem::take(&mut app.actions);
        for action in actions {
            apply(&mut app, action, &mut window, audio_sdl.as_ref());
        }
        // 4. Emulation.
        if let Some(session) = &mut app.session {
            let held = app.input.time_speed();
            session.scrub = if app.scrub_fraction != 0.0 { crate::scrub::speed(app.scrub_fraction) } else { held };
            session.paused = app.panel != Panel::None || app.dialog.is_some() || app.busy || app.wizard.is_some();
            let (p1, p2) = if session.paused { (nes_core::Buttons(0), nes_core::Buttons(0)) } else { app.input.buttons() };
            let advanced = session.advance(p1, p2);
            if advanced.hit_end {
                app.notice(END_OF_TAPE);
            }
            if let Some(e) = session.take_error() {
                app.library.log_problem(&session.game.title, &e);
            }
            app.rewind_depth = session.engine.rewind_depth();
            app.audio_ms = session.audio_ms();
            last_frame_uploaded = false;
        }
        // 5. Picture and panels.
        let (dw, dh) = window.drawable_size();
        unsafe {
            use glow::HasContext;
            video.gl().viewport(0, 0, dw as i32, dh as i32);
            video.gl().clear_color(0.067, 0.094, 0.075, 1.0);
            video.gl().clear(glow::COLOR_BUFFER_BIT);
        }
        if let Some(session) = &app.session {
            if !last_frame_uploaded {
                video.upload(session.engine.frame());
                last_frame_uploaded = true;
            }
            let scale = Bridge::pixels_per_point(&window);
            let top = if app.fullscreen { 0.0 } else { TITLE_BAR * scale };
            let bottom = if app.fullscreen { 0.0 } else { TIME_ROW * scale };
            let area = Viewport { x: 0, y: bottom as i32, width: dw as i32, height: (dh as f32 - top - bottom) as i32 };
            video.draw(area, app.settings.look, app.settings.aspect, app.settings.trim_edges);
        }
        bridge.paint(&window, &primitives, &mut textures);
        window.gl_swap_window();
        // 6. Housekeeping.
        if app.settings_dirty {
            if let Err(e) = app.settings.save(&app.data_dir) {
                app.report("Settings couldn't be saved.", &e);
            }
            app.settings_dirty = false;
        }
        if app.input.take_dirty() {
            if let Err(e) = app.input.profiles().save(&app.data_dir) {
                app.report("Controller buttons couldn't be saved.", &e);
            }
        }
        if let Some((_, since)) = app.notice {
            if since.elapsed() > NOTICE_TIME {
                app.notice = None;
            }
        }
        if app.fullscreen && app.session.is_some() {
            sdl.mouse().show_cursor(Instant::now() < app.chrome_until);
        } else {
            sdl.mouse().show_cursor(true);
        }
        if app.session.as_ref().is_some_and(|s| !s.paused) && app.session.as_ref().unwrap().until_due() > Duration::from_millis(1) && options.mute {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    if let Some(mut session) = app.session.take() {
        session.close();
    }
    video.destroy();
    bridge.destroy();
    Ok(())
}
```

Ordering note for the loop: egui must be painted *after* the GL picture, but `end_pass` must run before actions mutate state. So `bridge.rs` provides two calls instead of the single `end` shown in its listing: `Bridge::finish(&mut self) -> (egui::PlatformOutput, Vec<egui::ClippedPrimitive>, egui::TexturesDelta)` (calls `end_pass` and `tessellate`) and `Bridge::paint(&mut self, window: &Window, primitives: &[egui::ClippedPrimitive], textures: &mut egui::TexturesDelta)` (calls `painter.paint_and_update_textures` with the drawable size). Write those two in place of `Bridge::end`.

`video.gl()` is a `pub fn gl(&self) -> &glow::Context` accessor to add to `Video`.

The `draw` function for this task draws only what exists so far:

```rust
fn draw(ctx: &egui::Context, app: &mut App, video: &mut Video) {
    if app.session.is_none() {
        crate::ui::shelf::show(ctx, app);        // Task 11; until then: an empty CentralPanel with the wordmark and "Drop a .nes file here"
    } else {
        crate::ui::play::show(ctx, app);         // Task 12; until then: nothing
    }
    crate::ui::panels::show(ctx, app);           // Task 13; until then: nothing
    crate::ui::settings::show(ctx, app, video);  // Task 14; until then: nothing
}
```

For this task, write the placeholder bodies inline in `shell.rs` (an `egui::CentralPanel` with a `ui.heading("EMULIA")` when no session), and replace them with the module calls as the tasks land.

Action handling:

```rust
fn escape(app: &App) -> Action {
    match (app.dialog.is_some(), app.panel, app.wizard.is_some(), app.session.is_some(), app.fullscreen) {
        (true, ..) => Action::CloseDialog,
        (_, _, true, _, _) => Action::CancelWizard,
        (_, Panel::Problems, ..) | (_, Panel::Settings, ..) | (_, Panel::Slots, ..) => Action::ClosePanel,
        (_, Panel::Pause, ..) => Action::Resume,
        (_, Panel::None, _, true, _) => Action::Pause,
        (_, Panel::None, _, false, true) => Action::ToggleFullscreen,
        _ => Action::CloseDialog,
    }
}

fn dropped(app: &App, path: PathBuf) -> Action {
    let image = path.extension().is_some_and(|e| matches!(e.to_string_lossy().to_lowercase().as_str(), "png" | "jpg" | "jpeg"));
    match (&app.session, image) {
        (Some(session), true) => Action::ChooseArtFrom(session.game.clone(), path),
        _ => Action::ImportFrom(path),
    }
}

fn finish_wizard(app: &mut App, key: String, profile: crate::settings::Profile) {
    let name = profile.name.clone();
    app.input.set_profile(key, profile);
    app.wizard = None;
    app.panel = Panel::None;
    app.message = Some(format!("Buttons saved for {name}. The directional pad and stick work automatically."));
}

fn open_game(app: &mut App, game: Game, sdl: Option<&sdl2::Sdl>) {
    let audio = match sdl {
        Some(sdl) => match crate::open_audio(sdl) {
            Ok(a) => Some(a),
            Err(e) => {
                app.report("Sound couldn't be started.", &e);
                None
            }
        },
        None => None,
    };
    match Session::open(&app.library, game, audio) {
        Ok((session, warnings)) => {
            app.session = Some(session);
            app.thumbs.clear();
            app.panel = if warnings.is_empty() { Panel::None } else { Panel::Pause };
            if let Some(w) = warnings.first() {
                app.message = Some(w.clone());
            }
        }
        Err(e) => app.report("That game couldn't be opened.", &e),
    }
}

fn apply(app: &mut App, action: Action, window: &mut sdl2::video::Window, sdl: Option<&sdl2::Sdl>) {
    match action {
        Action::OpenGame(game) => open_game(app, game, sdl),
        Action::ImportFrom(path) => match crate::import(&app.library, &path) {
            Ok(game) => open_game(app, game, sdl),
            Err(e) => app.report("That game file didn't work.", &e),
        },
        Action::Pause => {
            if let Some(session) = &mut app.session {
                session.paused = true;
                session.record_playtime();
                if let Err(e) = session.save(Slot::Auto) {
                    let title = session.game.title.clone();
                    app.report("Progress couldn't be saved automatically.", &format!("{title}: {e}"));
                }
                app.thumbs.clear();
                app.panel = Panel::Pause;
                app.input.clear();
            }
        }
        Action::Resume => {
            app.panel = Panel::None;
            app.message = None;
        }
        Action::ClosePanel => app.panel = if app.session.is_some() { Panel::Pause } else { Panel::None },
        Action::CloseDialog => {
            app.dialog = None;
            app.message = None;
        }
        Action::ToggleFullscreen => {
            app.fullscreen = !app.fullscreen;
            app.settings.fullscreen = app.fullscreen;
            app.settings_dirty = true;
            let _ = window.set_fullscreen(if app.fullscreen { FullscreenType::Desktop } else { FullscreenType::Off });
        }
        Action::SaveConfirmed(n) => {
            if let Some(session) = &mut app.session {
                if let Err(e) = session.save(Slot::Number(n)) {
                    app.report("That save didn't work.", &e);
                }
                app.thumbs.remove(&n);
            }
            app.dialog = None;
        }
        Action::Load(n) => {
            if let Some(session) = &mut app.session {
                match session.load(Slot::Number(n)) {
                    Ok(()) => app.message = Some("Save loaded. Press Resume when you're ready.".into()),
                    Err(e) => app.report("That save couldn't be loaded.", &e),
                }
            }
        }
        Action::JumpBack(seconds) => {
            if let Some(session) = &mut app.session {
                let frames = session.engine.frames_for(seconds);
                let mut done = 0;
                while done < frames && session.engine.rewind_step() {
                    done += 1;
                }
                if done < frames {
                    app.notice(END_OF_TAPE);
                }
            }
        }
        Action::Scrub(fraction) => app.scrub_fraction = fraction,
        Action::ScrubReleased => app.scrub_fraction = 0.0,
        Action::Screenshot => {
            if let Some(session) = &app.session {
                match crate::files::pictures_dir().and_then(|dir| session.screenshot(&dir).map(|_| dir)) {
                    Ok(dir) => app.message = Some(format!("Screenshot saved to {}.", dir.display())),
                    Err(e) => app.report("The screenshot couldn't be saved.", &e),
                }
            }
        }
        Action::BackToShelf => {
            if let Some(mut session) = app.session.take() {
                session.close();
            }
            app.panel = Panel::None;
            app.dialog = None;
            app.scrub_fraction = 0.0;
            app.input.clear();
        }
        other => eprintln!("unhandled {other:?}"),
    }
}
```

Add `ImportFrom(PathBuf)` and `ChooseArtFrom(Game, PathBuf)` to `Action` in `ui/mod.rs`.

- [ ] **Step 4: Build and run it**

Run: `cargo build -p nes-desktop && cargo clippy -p nes-desktop --all-targets --no-deps -- -D warnings`

Then open a ROM you own: `cargo run --release -p nes-desktop -- "/path/to/game.nes" --data-dir /tmp/emulia-dev`. Expected: a window with the game running in the letterboxed area, sound, keyboard control, Space pauses (the picture freezes and the sound stops; there is no panel yet), F11 fullscreen, F5 then F8 restores, `.` fast-forwards, `,` rewinds, Backspace jumps back, closing the window writes `library/<hash>/auto.state` and `battery.sav`.

Run `cargo test -p nes-desktop` and `python3 scripts/check-desktop.py target/release/nes-desktop` again: still green.

- [ ] **Step 5: Commit**

```bash
git add desktop/src
git commit -m "Open the desktop window on OpenGL with an egui bridge and a playable loop"
```

---

### Task 10: Theme and widgets

**Files:**
- Create: `desktop/src/ui/theme.rs`, `desktop/src/ui/widgets.rs`
- Modify: `desktop/src/ui/mod.rs` (uncomment the two modules)

**Interfaces:**
- Produces (`theme.rs`): `pub fn apply(ctx: &egui::Context)`, and colour constants as `egui::Color32`: `BACKGROUND (#111813)`, `SURFACE (#1d2820)`, `SURFACE_HIGH (#243128)`, `ON_SURFACE (#edf4e9)`, `ON_SURFACE_VARIANT (#b3c4ad)`, `OUTLINE (#6d7f68)`, `LEAF (#b9e38c)`, `ON_LEAF (#17300c)`, `RAISED (#2a3a2e)`, `ON_RAISED (#c3d1bd)`, `ERROR (#ffb4a6)`, `ON_ERROR (#5f1409)`, `AMBER (#f0d49a)`, `ON_TIME (#152210)`, `NOTICE_BACK (#2b2410)`, `WELL (#141d17)`, `WELL_EDGE (#3b4d3e)`, `WELL_MARK (#5f7359)`, `KEY_DIM (#1b241d)`, `KEY_DIM_GLYPH (#54654f)`, `CHROME (#0a120c @ 0xcc)`, `SCRIM (#0a120c @ 0xd9)`, `COVER_TOP (#3b5a43)`, `COVER_BOTTOM (#21301f)`; sizes `CORNER_SMALL = 14.0`, `CORNER_MEDIUM = 20.0`, `CORNER_LARGE = 28.0`, `PRIMARY_HEIGHT = 60.0`, `SECONDARY_HEIGHT = 52.0`, `QUIET_HEIGHT = 46.0`, `PANEL_WIDTH = 620.0`
- Produces (`widgets.rs`):
  - `pub fn primary(ui: &mut egui::Ui, label: &str) -> egui::Response` (full width, `PRIMARY_HEIGHT`, LEAF fill, ON_LEAF text 17 semibold)
  - `pub fn secondary(ui, label) -> Response` (full width, `SECONDARY_HEIGHT`, RAISED fill, ON_RAISED text 15)
  - `pub fn quiet(ui, label, danger: bool) -> Response` (full width, `QUIET_HEIGHT`, transparent with OUTLINE stroke; ERROR text when `danger`)
  - `pub fn compact(ui, label, enabled: bool) -> Response` (auto width, 36 high, RAISED)
  - `pub fn tile(ui, label, width: f32) -> Response` (square-ish action tile: SURFACE_HIGH, `CORNER_MEDIUM`, 96 high)
  - `pub fn panel(ctx: &egui::Context, id: &str, title: &str, subtitle: Option<&str>, back: bool, add: impl FnOnce(&mut egui::Ui)) -> bool` (scrim over the whole window, centred card at most `PANEL_WIDTH` wide and 90% of the height with a vertical scroll area, title 27 bold, optional subtitle, optional back arrow; returns true when the back arrow was clicked)
  - `pub fn section(ui, label)` (uppercase, 12, bold, +2 letter spacing, LEAF)
  - `pub fn choice_row<T: Copy + PartialEq>(ui, items: &[T], selected: T, label: impl Fn(T) -> &'static str) -> Option<T>` (horizontally scrolling chips; the chosen one filled LEAF)
  - `pub fn note(ui, text)` (12, ON_SURFACE_VARIANT)
  - `pub fn value_row(ui, title: &str, hint: &str, action: &str) -> egui::Response` (title + hint left, compact button right)
  - `pub fn info_row(ui, title: &str, value: &str, hint: &str)`
  - `pub fn switch_row(ui, title: &str, hint: &str, on: &mut bool) -> bool` (returns true when changed)
  - `pub fn pill(ui, text: &str, back: Color32, fore: Color32)` (rounded notice)
  - `pub fn message_bar(ctx, text: &str) -> bool` (a bottom-centre dismissable sentence with an "OK" compact button; returns true when dismissed)

- [ ] **Step 1: Write theme.rs**

```rust
//! The Android design system's tokens, applied to egui. Three greens and
//! everything descends from them; three radii; three button heights.

use egui::{Color32, CornerRadius, Stroke};

pub const BACKGROUND: Color32 = Color32::from_rgb(0x11, 0x18, 0x13);
pub const SURFACE: Color32 = Color32::from_rgb(0x1d, 0x28, 0x20);
pub const SURFACE_HIGH: Color32 = Color32::from_rgb(0x24, 0x31, 0x28);
pub const ON_SURFACE: Color32 = Color32::from_rgb(0xed, 0xf4, 0xe9);
pub const ON_SURFACE_VARIANT: Color32 = Color32::from_rgb(0xb3, 0xc4, 0xad);
pub const OUTLINE: Color32 = Color32::from_rgb(0x6d, 0x7f, 0x68);
pub const LEAF: Color32 = Color32::from_rgb(0xb9, 0xe3, 0x8c);
pub const ON_LEAF: Color32 = Color32::from_rgb(0x17, 0x30, 0x0c);
pub const RAISED: Color32 = Color32::from_rgb(0x2a, 0x3a, 0x2e);
pub const ON_RAISED: Color32 = Color32::from_rgb(0xc3, 0xd1, 0xbd);
pub const ERROR: Color32 = Color32::from_rgb(0xff, 0xb4, 0xa6);
pub const ON_ERROR: Color32 = Color32::from_rgb(0x5f, 0x14, 0x09);
pub const AMBER: Color32 = Color32::from_rgb(0xf0, 0xd4, 0x9a);
pub const ON_TIME: Color32 = Color32::from_rgb(0x15, 0x22, 0x10);
pub const NOTICE_BACK: Color32 = Color32::from_rgb(0x2b, 0x24, 0x10);
pub const WELL: Color32 = Color32::from_rgb(0x14, 0x1d, 0x17);
pub const WELL_EDGE: Color32 = Color32::from_rgb(0x3b, 0x4d, 0x3e);
pub const WELL_MARK: Color32 = Color32::from_rgb(0x5f, 0x73, 0x59);
pub const KEY_DIM: Color32 = Color32::from_rgb(0x1b, 0x24, 0x1d);
pub const KEY_DIM_GLYPH: Color32 = Color32::from_rgb(0x54, 0x65, 0x4f);
pub const CHROME: Color32 = Color32::from_rgba_premultiplied(0x0a, 0x12, 0x0c, 0xcc);
pub const SCRIM: Color32 = Color32::from_rgba_premultiplied(0x0a, 0x12, 0x0c, 0xd9);
pub const COVER_TOP: Color32 = Color32::from_rgb(0x3b, 0x5a, 0x43);
pub const COVER_BOTTOM: Color32 = Color32::from_rgb(0x21, 0x30, 0x1f);

pub const CORNER_SMALL: f32 = 14.0;
pub const CORNER_MEDIUM: f32 = 20.0;
pub const CORNER_LARGE: f32 = 28.0;
pub const PRIMARY_HEIGHT: f32 = 60.0;
pub const SECONDARY_HEIGHT: f32 = 52.0;
pub const QUIET_HEIGHT: f32 = 46.0;
pub const PANEL_WIDTH: f32 = 620.0;

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let v = &mut style.visuals;
    v.dark_mode = true;
    v.override_text_color = Some(ON_SURFACE);
    v.panel_fill = BACKGROUND;
    v.window_fill = SURFACE;
    v.extreme_bg_color = WELL;
    v.faint_bg_color = SURFACE_HIGH;
    v.window_corner_radius = CornerRadius::same(CORNER_LARGE as u8);
    v.window_stroke = Stroke::NONE;
    v.selection.bg_fill = LEAF;
    v.selection.stroke = Stroke::new(1.0, ON_LEAF);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.bg_fill = RAISED;
        w.weak_bg_fill = SURFACE_HIGH;
        w.bg_stroke = Stroke::NONE;
        w.corner_radius = CornerRadius::same(CORNER_SMALL as u8);
        w.fg_stroke = Stroke::new(1.0, ON_RAISED);
    }
    v.widgets.hovered.bg_fill = SURFACE_HIGH;
    v.widgets.active.bg_fill = LEAF;
    v.widgets.active.fg_stroke = Stroke::new(1.0, ON_LEAF);
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(16.0, 10.0);
    ctx.set_style(style);
}
```

Verify field names against `egui-0.36.2/src/style.rs` (`panel_fill`, `window_fill`, `selection`, `widgets.*.fg_stroke`, `corner_radius` of type `CornerRadius` with `u8` components).

- [ ] **Step 2: Write widgets.rs**

```rust
//! The shell's handful of controls, drawn to the Android tokens. Each is a
//! plain function that allocates a rect, paints it and returns the response,
//! so every screen is built from the same few shapes.

use super::theme::*;
use egui::{Align2, Color32, CornerRadius, FontId, Rect, Response, Sense, Stroke, Ui, Vec2};

fn label_button(ui: &mut Ui, label: &str, size: Vec2, fill: Color32, fore: Color32, stroke: Stroke, font: f32, radius: f32, enabled: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    if ui.is_rect_visible(rect) {
        let fill = if !enabled { KEY_DIM } else if response.is_pointer_button_down_on() { fill.gamma_multiply(1.15) } else if response.hovered() { fill.gamma_multiply(1.08) } else { fill };
        let fore = if enabled { fore } else { KEY_DIM_GLYPH };
        ui.painter().rect(rect, CornerRadius::same(radius as u8), fill, stroke, egui::StrokeKind::Inside);
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(font), fore);
    }
    response
}

pub fn primary(ui: &mut Ui, label: &str) -> Response {
    let size = Vec2::new(ui.available_width(), PRIMARY_HEIGHT);
    label_button(ui, label, size, LEAF, ON_LEAF, Stroke::NONE, 17.0, CORNER_MEDIUM, true)
}

pub fn secondary(ui: &mut Ui, label: &str) -> Response {
    let size = Vec2::new(ui.available_width(), SECONDARY_HEIGHT);
    label_button(ui, label, size, RAISED, ON_RAISED, Stroke::NONE, 15.0, CORNER_MEDIUM, true)
}

pub fn quiet(ui: &mut Ui, label: &str, danger: bool) -> Response {
    let size = Vec2::new(ui.available_width(), QUIET_HEIGHT);
    let fore = if danger { ERROR } else { ON_RAISED };
    label_button(ui, label, size, Color32::TRANSPARENT, fore, Stroke::new(1.0, OUTLINE), 15.0, CORNER_SMALL, true)
}

pub fn compact(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    let width = ui.fonts(|f| f.layout_no_wrap(label.to_string(), FontId::proportional(14.0), ON_RAISED).size().x) + 28.0;
    label_button(ui, label, Vec2::new(width, 36.0), RAISED, ON_RAISED, Stroke::NONE, 14.0, CORNER_SMALL, enabled)
}

pub fn tile(ui: &mut Ui, label: &str, width: f32) -> Response {
    label_button(ui, label, Vec2::new(width, 96.0), SURFACE_HIGH, ON_SURFACE, Stroke::NONE, 15.0, CORNER_MEDIUM, true)
}

pub fn section(ui: &mut Ui, label: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(label.to_uppercase()).size(12.0).strong().extra_letter_spacing(2.0).color(LEAF));
}

pub fn note(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(12.0).color(ON_SURFACE_VARIANT));
}

pub fn choice_row<T: Copy + PartialEq>(ui: &mut Ui, items: &[T], selected: T, label: impl Fn(T) -> &'static str) -> Option<T> {
    let mut chosen = None;
    egui::ScrollArea::horizontal().id_salt(ui.next_auto_id()).show(ui, |ui| {
        ui.horizontal(|ui| {
            for &item in items {
                let on = item == selected;
                let (fill, fore) = if on { (LEAF, ON_LEAF) } else { (SURFACE_HIGH, ON_RAISED) };
                let text = label(item);
                let width = ui.fonts(|f| f.layout_no_wrap(text.to_string(), FontId::proportional(14.0), fore).size().x) + 28.0;
                if label_button(ui, text, Vec2::new(width, 36.0), fill, fore, Stroke::NONE, 14.0, CORNER_SMALL, true).clicked() {
                    chosen = Some(item);
                }
            }
        });
    });
    chosen
}

fn row_body(ui: &mut Ui, title: &str, hint: &str) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(title).size(16.0));
        if !hint.is_empty() {
            note(ui, hint);
        }
    });
}

pub fn value_row(ui: &mut Ui, title: &str, hint: &str, action: &str) -> Response {
    let mut response = None;
    egui::Frame::new().fill(SURFACE_HIGH).corner_radius(CornerRadius::same(CORNER_SMALL as u8)).inner_margin(14.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            row_body(ui, title, hint);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                response = Some(compact(ui, action, true));
            });
        });
    });
    response.unwrap()
}

pub fn info_row(ui: &mut Ui, title: &str, value: &str, hint: &str) {
    egui::Frame::new().fill(SURFACE_HIGH).corner_radius(CornerRadius::same(CORNER_SMALL as u8)).inner_margin(14.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            row_body(ui, title, hint);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(value).size(15.0).color(ON_RAISED));
            });
        });
    });
}

pub fn switch_row(ui: &mut Ui, title: &str, hint: &str, on: &mut bool) -> bool {
    let mut changed = false;
    egui::Frame::new().fill(SURFACE_HIGH).corner_radius(CornerRadius::same(CORNER_SMALL as u8)).inner_margin(14.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            row_body(ui, title, hint);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (rect, response) = ui.allocate_exact_size(Vec2::new(52.0, 30.0), Sense::click());
                if response.clicked() {
                    *on = !*on;
                    changed = true;
                }
                let fill = if *on { LEAF } else { WELL };
                ui.painter().rect_filled(rect, 15.0, fill);
                let knob = if *on { rect.right_center() - Vec2::new(15.0, 0.0) } else { rect.left_center() + Vec2::new(15.0, 0.0) };
                ui.painter().circle_filled(knob, 11.0, if *on { ON_LEAF } else { ON_RAISED });
            });
        });
    });
    changed
}

pub fn pill(ui: &mut Ui, text: &str, back: Color32, fore: Color32) {
    egui::Frame::new().fill(back).corner_radius(CornerRadius::same(18)).inner_margin(egui::Margin::symmetric(16, 8)).show(ui, |ui| {
        ui.label(egui::RichText::new(text).size(14.0).strong().color(fore));
    });
}

/// A scrim and one card in the middle of the window. Returns true when the
/// back arrow was pressed.
pub fn panel(ctx: &egui::Context, id: &str, title: &str, subtitle: Option<&str>, back: bool, add: impl FnOnce(&mut Ui)) -> bool {
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new((id, "scrim"))).order(egui::Order::Middle).fixed_pos(screen.min).interactable(true).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
        ui.painter().rect_filled(rect, 0.0, SCRIM);
    });
    let mut went_back = false;
    let width = PANEL_WIDTH.min(screen.width() - 32.0);
    let max_height = screen.height() * 0.9;
    egui::Area::new(egui::Id::new((id, "card"))).order(egui::Order::Foreground).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
        egui::Frame::new().fill(SURFACE).corner_radius(CornerRadius::same(CORNER_LARGE as u8)).inner_margin(24.0).show(ui, |ui| {
            ui.set_width(width - 48.0);
            ui.set_max_height(max_height - 48.0);
            ui.horizontal(|ui| {
                if back && compact(ui, "←", true).clicked() {
                    went_back = true;
                }
                ui.label(egui::RichText::new(title).size(27.0).strong());
            });
            if let Some(subtitle) = subtitle {
                note(ui, subtitle);
            }
            ui.add_space(8.0);
            egui::ScrollArea::vertical().max_height(max_height - 140.0).show(ui, |ui| {
                ui.set_width(width - 64.0);
                add(ui);
            });
        });
    });
    went_back
}

/// The plain sentence, bottom centre, until dismissed.
pub fn message_bar(ctx: &egui::Context, text: &str) -> bool {
    let mut dismissed = false;
    egui::Area::new(egui::Id::new("message")).order(egui::Order::Tooltip).anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -24.0)).show(ctx, |ui| {
        egui::Frame::new().fill(SURFACE_HIGH).corner_radius(CornerRadius::same(CORNER_MEDIUM as u8)).inner_margin(16.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(text).size(15.0));
                if compact(ui, "OK", true).clicked() {
                    dismissed = true;
                }
            });
        });
    });
    dismissed
}

pub fn cover_placeholder(ui: &mut Ui, rect: Rect, title: &str) {
    let painter = ui.painter();
    painter.rect_filled(rect, CORNER_SMALL, COVER_BOTTOM);
    painter.rect_filled(Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.center().y)), CornerRadius { nw: CORNER_SMALL as u8, ne: CORNER_SMALL as u8, sw: 0, se: 0 }, COVER_TOP);
    let initial: String = title.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
    painter.text(rect.center(), Align2::CENTER_CENTER, initial, FontId::proportional(rect.height() * 0.5), ON_SURFACE);
}
```

Check against egui 0.36: `Painter::rect` signature takes `StrokeKind` (it does in 0.31+); `Frame::new()` exists (0.31+; otherwise `Frame::none()`); `CornerRadius` components are `u8`; `ScrollArea::id_salt` exists (0.29+); `Color32::gamma_multiply` exists.

- [ ] **Step 3: Wire the theme and a smoke placeholder**

`shell::run` already calls `crate::ui::theme::apply(&bridge.ctx)`. In the placeholder `draw`, render `widgets::message_bar` when `app.message` is set, pushing `Action::CloseDialog` when dismissed, so the message plumbing is exercised now.

Run: `cargo build -p nes-desktop && cargo clippy -p nes-desktop --all-targets --no-deps -- -D warnings`, then run the app with a bad ROM path to see the message bar: `cargo run --release -p nes-desktop -- /etc/hosts --data-dir /tmp/emulia-dev`. Expected: the window opens with "That game file didn't work." and an OK button that dismisses it.

- [ ] **Step 4: Commit**

```bash
git add desktop/src/ui
git commit -m "Add the desktop theme tokens and the shell's widget set"
```

---

### Task 11: The shelf

**Files:**
- Create: `desktop/src/ui/shelf.rs`
- Modify: `desktop/src/ui/mod.rs` (uncomment `shelf`), `desktop/src/shell.rs` (call `shelf::show`; handle `Import`, `ChooseArt`, `ChooseArtFrom`, `ClearArt`, `SetArchived`, `DeleteRequested`, `DeleteConfirmed`, `ShowArchive`, `ToggleShelfList`)

**Interfaces:**
- Consumes: `App`, `widgets`, `theme`, `library::{Game, playtime}`, `rfd::FileDialog`
- Produces: `pub fn show(ctx: &egui::Context, app: &mut App)`

- [ ] **Step 1: Write shelf.rs**

```rust
//! The shelf: what you have, most recently played first, and the way in.

use super::theme::*;
use super::widgets;
use super::Action;
use crate::library::{playtime, Game};
use crate::shell::App;
use egui::{Align2, CornerRadius, FontId, Rect, Sense, Vec2};

const TIGHT: f32 = 600.0;
const ROOMY: f32 = 820.0;

pub fn show(ctx: &egui::Context, app: &mut App) {
    egui::CentralPanel::default().frame(egui::Frame::new().fill(BACKGROUND).inner_margin(24.0)).show(ctx, |ui| {
        let width = ui.available_width();
        let (tight, roomy) = (width < TIGHT, width >= ROOMY);
        let games = if app.show_archive { app.library.archived() } else { app.library.games() };
        let archived = app.library.archived().len();
        if roomy {
            ui.horizontal(|ui| {
                heading(ui, app.show_archive);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| actions(ui, app, archived, !games.is_empty()));
            });
        } else {
            heading(ui, app.show_archive);
            egui::ScrollArea::horizontal().id_salt("shelf-actions").show(ui, |ui| ui.horizontal(|ui| actions(ui, app, archived, !games.is_empty())));
        }
        ui.add_space(16.0);
        if games.is_empty() {
            empty(ui, app.show_archive);
            return;
        }
        let list = app.settings.shelf_list;
        egui::ScrollArea::vertical().id_salt("shelf").show(ui, |ui| {
            if list {
                for game in &games {
                    row(ui, app, game);
                }
            } else {
                let card_width = if tight { 150.0 } else { 220.0 };
                let columns = ((ui.available_width() + 12.0) / (card_width + 12.0)).floor().max(1.0) as usize;
                for chunk in games.chunks(columns) {
                    ui.horizontal_top(|ui| {
                        for game in chunk {
                            card(ui, app, game, card_width);
                        }
                    });
                }
            }
        });
    });
}

fn heading(ui: &mut egui::Ui, archive: bool) {
    let text = if archive { "Put away" } else { "EMULIA" };
    let rich = egui::RichText::new(text).size(20.0).strong().color(LEAF);
    ui.label(if archive { rich } else { rich.extra_letter_spacing(3.0) });
}

fn actions(ui: &mut egui::Ui, app: &mut App, archived: usize, any_games: bool) {
    if app.show_archive {
        if widgets::compact(ui, "Back to the shelf", true).clicked() {
            app.actions.push(Action::ShowArchive(false));
        }
        return;
    }
    if any_games {
        let label = if app.settings.shelf_list { "Cards" } else { "List" };
        if widgets::compact(ui, label, true).clicked() {
            app.actions.push(Action::ToggleShelfList);
        }
    }
    if archived > 0 && widgets::compact(ui, &format!("Put away ({archived})"), true).clicked() {
        app.actions.push(Action::ShowArchive(true));
    }
    if widgets::compact(ui, "Settings", true).clicked() {
        app.actions.push(Action::OpenSettings);
    }
    if widgets::compact(ui, "Add a game", true).clicked() {
        app.actions.push(Action::Import);
    }
}

fn empty(ui: &mut egui::Ui, archive: bool) {
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.25);
        ui.label(egui::RichText::new("EMULIA").size(44.0).strong().color(LEAF).extra_letter_spacing(6.0));
        ui.add_space(12.0);
        if archive {
            ui.label(egui::RichText::new("Nothing is put away.").size(20.0));
        } else {
            ui.label(egui::RichText::new("A shelf full of possibilities").size(20.0));
            widgets::note(ui, "Add a game file (.nes) from your computer to begin.");
            widgets::note(ui, "Games stay on this device. No account needed.");
        }
    });
}

fn cover(ui: &mut egui::Ui, app: &mut App, game: &Game, rect: Rect) {
    match app.cover_texture(ui.ctx(), game) {
        Some(texture) => {
            // Cropped, not fitted: fill the 4:3 box from the centre.
            let size = texture.size_vec2();
            let scale = (rect.width() / size.x).max(rect.height() / size.y);
            let shown = size * scale;
            let u = ((shown.x - rect.width()) / shown.x) / 2.0;
            let v = ((shown.y - rect.height()) / shown.y) / 2.0;
            let uv = Rect::from_min_max(egui::pos2(u, v), egui::pos2(1.0 - u, 1.0 - v));
            ui.painter().image(texture.id(), rect, uv, egui::Color32::WHITE);
        }
        None => widgets::cover_placeholder(ui, rect, &game.title),
    }
}

fn status(app: &App, game: &Game) -> &'static str {
    if app.library.has_autosave(&game.id) { "Resume" } else { "Ready to play" }
}

fn menu(ui: &mut egui::Ui, app: &mut App, game: &Game) {
    let id = ui.make_persistent_id(("menu", &game.id));
    let response = widgets::compact(ui, "⋯", true);
    if response.clicked() {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    egui::popup::popup_below_widget(ui, id, &response, egui::PopupCloseBehavior::CloseOnClick, |ui| {
        ui.set_min_width(180.0);
        if app.show_archive {
            if ui.button("Bring back").clicked() {
                app.actions.push(Action::SetArchived(game.clone(), false));
            }
            if ui.button(egui::RichText::new("Delete").color(ERROR)).clicked() {
                app.actions.push(Action::DeleteRequested(game.clone()));
            }
        } else {
            if ui.button("Choose box art").clicked() {
                app.actions.push(Action::ChooseArt(game.clone()));
            }
            if app.library.art_path(&game.id).exists() && ui.button("Clear box art").clicked() {
                app.actions.push(Action::ClearArt(game.clone()));
            }
            if ui.button("Put this away").clicked() {
                app.actions.push(Action::SetArchived(game.clone(), true));
            }
        }
    });
}

fn card(ui: &mut egui::Ui, app: &mut App, game: &Game, width: f32) {
    egui::Frame::new().fill(SURFACE).corner_radius(CornerRadius::same(CORNER_MEDIUM as u8)).inner_margin(12.0).show(ui, |ui| {
        ui.set_width(width - 24.0);
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width - 24.0, (width - 24.0) * 0.75), Sense::click());
        cover(ui, app, game, rect);
        if response.clicked() {
            app.actions.push(Action::OpenGame(game.clone()));
        }
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(width - 90.0);
                ui.label(egui::RichText::new(&game.title).size(16.0).strong());
                ui.label(egui::RichText::new(format!("{} →", status(app, game))).size(13.0).color(LEAF));
                if let Some(time) = playtime(game.seconds) {
                    widgets::note(ui, &time);
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| menu(ui, app, game));
        });
    });
}

fn row(ui: &mut egui::Ui, app: &mut App, game: &Game) {
    egui::Frame::new().fill(SURFACE).corner_radius(CornerRadius::same(CORNER_SMALL as u8)).inner_margin(10.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let (rect, response) = ui.allocate_exact_size(Vec2::new(76.0 * 4.0 / 3.0, 76.0), Sense::click());
            cover(ui, app, game, rect);
            let title = ui.add(egui::Label::new(egui::RichText::new(&game.title).size(16.0).strong()).sense(Sense::click()));
            if response.clicked() || title.clicked() {
                app.actions.push(Action::OpenGame(game.clone()));
            }
            let mut subtitle = status(app, game).to_string();
            if let Some(time) = playtime(game.seconds) {
                subtitle = format!("{subtitle} · {time}");
            }
            widgets::note(ui, &subtitle);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| menu(ui, app, game));
        });
    });
}
```

Check `egui::popup::popup_below_widget` and `PopupCloseBehavior` exist in 0.36; if the popup API moved to `egui::Popup::menu(&response).show(...)`, use that instead (0.31+ introduced `egui::Popup`). Suppress the unused variable warnings for `FontId`/`Align2` if they end up unused.

- [ ] **Step 2: Handle the shelf actions in shell.rs**

Add to `apply`:

```rust
        Action::Import => {
            if let Some(path) = rfd::FileDialog::new().add_filter("NES game", &["nes"]).set_title("Add a game").pick_file() {
                apply(app, Action::ImportFrom(path), window, sdl);
            }
        }
        Action::ChooseArt(game) => {
            if let Some(path) = rfd::FileDialog::new().add_filter("Image", &["png", "jpg", "jpeg"]).set_title("Choose box art").pick_file() {
                apply(app, Action::ChooseArtFrom(game, path), window, sdl);
            }
        }
        Action::ChooseArtFrom(game, path) => match app.library.set_art(&game.id, &path) {
            Ok(()) => { app.covers.remove(&game.id); }
            Err(e) => app.report("That picture didn't work as box art.", &e),
        },
        Action::ClearArt(game) => {
            if let Err(e) = app.library.clear_art(&game.id) {
                app.report("The box art couldn't be cleared.", &e);
            }
            app.covers.remove(&game.id);
        }
        Action::SetArchived(game, archived) => {
            if let Err(e) = app.library.set_archived(&game.id, archived) {
                app.report("The shelf couldn't be updated.", &e);
            }
        }
        Action::DeleteRequested(game) => app.dialog = Some(Dialog::ConfirmDelete(game)),
        Action::DeleteConfirmed(game) => {
            if let Err(e) = app.library.forget(&game.id) {
                app.report("The game couldn't be deleted.", &e);
            }
            app.covers.remove(&game.id);
            app.dialog = None;
        }
        Action::ShowArchive(show) => app.show_archive = show,
        Action::ToggleShelfList => {
            app.settings.shelf_list = !app.settings.shelf_list;
            app.settings_dirty = true;
        }
```

`rfd` blocking dialogs on macOS must run on the main thread: they do here. On Linux `rfd` uses GTK or the XDG portal; `rfd` 0.17 default features include `gtk3`, which needs `libgtk-3-dev` at build time. Prefer the portal: set `rfd = { version = "0.17", default-features = false, features = ["xdg-portal", "tokio"] }` on Linux only, via `[target.'cfg(target_os = "linux")'.dependencies]` and `[target.'cfg(not(target_os = "linux"))'.dependencies]` in `desktop/Cargo.toml`, and add `xdg-desktop-portal` to the Linux notes in `docs/DESKTOP.md`. If the portal feature needs an async runtime, use `rfd`'s `AsyncFileDialog` with `pollster::block_on` (add `pollster = "0.4"`). Verify which combination builds on the Ubuntu CI image; the desktop workflow installs no GTK.

Also in the shelf `draw` placeholder: replace it with `crate::ui::shelf::show(ctx, app)`.

- [ ] **Step 3: Build, run, verify**

Run: `cargo build -p nes-desktop && cargo clippy -p nes-desktop --all-targets --no-deps -- -D warnings` then `cargo run --release -p nes-desktop -- --data-dir /tmp/emulia-dev`.
Expected: the shelf shows the game imported in Task 9 with its autosave thumbnail as cover, Resume → and playtime; Add a game opens a native file dialog and imports; the card menu sets and clears box art (drag an image onto the window while a game is open also sets it); Put this away moves it to the archive and Put away (1) shows it with Bring back and Delete; the grid/list toggle persists across restarts; clicking a card opens the game.

- [ ] **Step 4: Commit**

```bash
git add desktop/src/ui/shelf.rs desktop/src/ui/mod.rs desktop/src/shell.rs desktop/Cargo.toml Cargo.lock
git commit -m "Add the desktop shelf with covers, import, box art and the archive"
```

---

### Task 12: The play view: title bar, chrome, time control and HUD

**Files:**
- Create: `desktop/src/ui/time.rs`, `desktop/src/ui/play.rs`
- Modify: `desktop/src/ui/mod.rs`, `desktop/src/shell.rs` (call `play::show`)

**Interfaces:**
- Produces (`time.rs`): `pub fn scrubber(ui: &mut egui::Ui, fraction: f32, width: f32) -> ScrubEvent` with `pub enum ScrubEvent { None, Dragged(f32), Released, Tapped }`; `pub fn skip_back(ui: &mut egui::Ui, seconds: u32, enabled: bool) -> bool`
- Produces (`play.rs`): `pub fn show(ctx: &egui::Context, app: &mut App)` drawing the title bar (not fullscreen), the chrome pill (fullscreen, while `Instant::now() < app.chrome_until`), the bottom time row (both, hidden with the chrome in fullscreen), the HUD notice pill and the busy scrim.

- [ ] **Step 1: Write time.rs**

```rust
//! One horizontal track for both directions of time. Drag left to rewind,
//! right to fast-forward, further is faster; release and it springs back.
//! A click on the handle without a drag pauses.

use super::theme::*;
use crate::scrub;
use egui::{Align2, CornerRadius, FontId, Rect, Sense, Stroke, Vec2};

pub const HEIGHT: f32 = 56.0;
const HANDLE: f32 = 64.0;

#[derive(Debug, PartialEq)]
pub enum ScrubEvent {
    None,
    Dragged(f32),
    Released,
    Tapped,
}

pub fn scrubber(ui: &mut egui::Ui, fraction: f32, width: f32) -> ScrubEvent {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, HEIGHT), Sense::click_and_drag());
    let painter = ui.painter();
    painter.rect(rect, CornerRadius::same((HEIGHT / 2.0) as u8), WELL, Stroke::NONE, egui::StrokeKind::Inside);
    painter.text(rect.left_center() + Vec2::new(18.0, 0.0), Align2::CENTER_CENTER, "◀◀", FontId::proportional(13.0), WELL_MARK);
    painter.text(rect.right_center() - Vec2::new(18.0, 0.0), Align2::CENTER_CENTER, "▶▶", FontId::proportional(13.0), WELL_MARK);
    painter.rect_filled(Rect::from_center_size(rect.center(), Vec2::new(2.0, HEIGHT - 16.0)), 1.0, WELL_EDGE);
    let travel = (width - HANDLE) / 2.0;
    let centre = rect.center() + Vec2::new(fraction.clamp(-1.0, 1.0) * travel, 0.0);
    let handle = Rect::from_center_size(centre, Vec2::new(HANDLE, HEIGHT - 8.0));
    let speed = scrub::speed(fraction);
    let (fill, fore) = if speed < 0 { (AMBER, ON_TIME) } else if speed > 0 { (LEAF, ON_TIME) } else { (RAISED, ON_RAISED) };
    painter.rect(handle, CornerRadius::same(((HEIGHT - 8.0) / 2.0) as u8), fill, Stroke::NONE, egui::StrokeKind::Inside);
    let glyph = if speed == 0 { "▮▮".to_string() } else { format!("{}×", speed.abs()) };
    painter.text(handle.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(15.0), fore);

    if response.dragged() || response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            return ScrubEvent::Dragged(((pos.x - rect.center().x) / travel).clamp(-1.0, 1.0));
        }
    }
    if response.drag_stopped() {
        return ScrubEvent::Released;
    }
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            if handle.contains(pos) {
                return ScrubEvent::Tapped;
            }
            // A press elsewhere on the track scrubs from that point; with no
            // drag it is a momentary nudge that the release cancels.
            return ScrubEvent::Released;
        }
    }
    ScrubEvent::None
}

pub fn skip_back(ui: &mut egui::Ui, seconds: u32, enabled: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(56.0, HEIGHT), if enabled { Sense::click() } else { Sense::hover() });
    let (fill, fore) = if enabled { (RAISED, ON_RAISED) } else { (KEY_DIM, KEY_DIM_GLYPH) };
    ui.painter().rect(rect, CornerRadius::same((HEIGHT / 2.0) as u8), fill, Stroke::NONE, egui::StrokeKind::Inside);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, format!("↺{seconds}"), FontId::proportional(15.0), fore);
    enabled && response.clicked()
}
```

- [ ] **Step 2: Write play.rs**

```rust
//! What sits around the picture while a game is open: a title bar with two
//! buttons, the time controls, one amber notice, and a chrome pill in full
//! screen that fades when the hands are still.

use super::theme::*;
use super::time::{scrubber, skip_back, ScrubEvent};
use super::widgets;
use super::{Action, Panel};
use crate::shell::{App, TIME_ROW, TITLE_BAR};
use egui::{Align2, Vec2};
use std::time::Instant;

pub fn show(ctx: &egui::Context, app: &mut App) {
    let Some(session) = &app.session else { return };
    let title = session.game.title.clone();
    let frame_rate = session.engine.frame_rate();
    let chrome_visible = !app.fullscreen || Instant::now() < app.chrome_until;

    if !app.fullscreen {
        egui::TopBottomPanel::top("title").exact_height(TITLE_BAR).frame(egui::Frame::new().fill(BACKGROUND).inner_margin(egui::Margin::symmetric(16, 8))).show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(egui::RichText::new(&title).size(17.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::compact(ui, "Menu", true).clicked() {
                        app.actions.push(Action::Pause);
                    }
                    if widgets::compact(ui, "Full screen", true).clicked() {
                        app.actions.push(Action::ToggleFullscreen);
                    }
                });
            });
        });
    } else if chrome_visible {
        egui::Area::new(egui::Id::new("chrome")).order(egui::Order::Foreground).anchor(Align2::RIGHT_TOP, Vec2::new(-16.0, 16.0)).show(ctx, |ui| {
            egui::Frame::new().fill(CHROME).corner_radius(24.0).inner_margin(8.0).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if widgets::compact(ui, "Menu", true).clicked() {
                        app.actions.push(Action::Pause);
                    }
                    if widgets::compact(ui, "Exit full screen", true).clicked() {
                        app.actions.push(Action::ToggleFullscreen);
                    }
                });
            });
        });
    }

    if chrome_visible && app.panel == Panel::None {
        let frame = if app.fullscreen { egui::Frame::new().fill(egui::Color32::TRANSPARENT) } else { egui::Frame::new().fill(BACKGROUND) };
        egui::TopBottomPanel::bottom("time").exact_height(TIME_ROW).frame(frame.inner_margin(egui::Margin::symmetric(16, 12))).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.horizontal(|ui| {
                    let total = 56.0 * 2.0 + 240.0 + 20.0;
                    ui.add_space((ui.available_width() - total).max(0.0) / 2.0);
                    for seconds in [5u32, 15] {
                        let enabled = app.rewind_depth >= (seconds as f64 * frame_rate) as usize;
                        if skip_back(ui, seconds, enabled) {
                            app.actions.push(Action::JumpBack(seconds));
                        }
                    }
                    match scrubber(ui, app.scrub_fraction, 240.0) {
                        ScrubEvent::Dragged(f) => app.actions.push(Action::Scrub(f)),
                        ScrubEvent::Released => app.actions.push(Action::ScrubReleased),
                        ScrubEvent::Tapped => app.actions.push(Action::Pause),
                        ScrubEvent::None => {}
                    }
                });
            });
        });
    }

    let speed = if app.scrub_fraction != 0.0 { crate::scrub::speed(app.scrub_fraction) } else { app.input.time_speed() };
    let hud = match &app.notice {
        Some((text, _)) => Some(text.clone()),
        None if speed != 0 && app.panel == Panel::None => Some(crate::scrub::label(speed)),
        None => None,
    };
    if let Some(text) = hud {
        egui::Area::new(egui::Id::new("hud")).order(egui::Order::Foreground).anchor(Align2::CENTER_TOP, Vec2::new(0.0, if app.fullscreen { 16.0 } else { TITLE_BAR + 12.0 })).interactable(false).show(ctx, |ui| {
            widgets::pill(ui, &text, NOTICE_BACK, AMBER);
        });
    }

    if app.busy {
        egui::Area::new(egui::Id::new("busy")).order(egui::Order::Tooltip).fixed_pos(ctx.screen_rect().min).show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(ctx.screen_rect().size(), egui::Sense::click());
            ui.painter().rect_filled(rect, 0.0, SCRIM);
            ui.put(egui::Rect::from_center_size(rect.center(), Vec2::splat(40.0)), egui::Spinner::new().color(LEAF));
        });
    }
}
```

The picture area computed in `shell.rs` (Task 9) already leaves `TITLE_BAR` and `TIME_ROW` when not fullscreen. In fullscreen the time row overlays the picture with a transparent frame.

- [ ] **Step 3: Wire it and check the scrub feel**

Replace the play placeholder in `shell::draw` with `crate::ui::play::show(ctx, app)`. Ensure `app.chrome_until` is also refreshed on `ScrubEvent` and skip clicks (pointer events already do it).

Run: `cargo build -p nes-desktop && cargo clippy -p nes-desktop --all-targets --no-deps -- -D warnings`, then run with a game. Expected: title bar with the two buttons; drag the handle left, the game rewinds with an amber "Rewinding N×" pill and the handle turns amber; drag right, green "Fast-forward N×"; release springs back and play resumes; ↺5 and ↺15 dim until enough history exists and jump back; at the end of the tape the "That's as far back as this goes." notice shows for about two seconds; F11 fullscreen hides everything after five idle seconds and a mouse move brings back the pill and the time row; Menu pauses (no panel yet: the picture freezes).

- [ ] **Step 4: Commit**

```bash
git add desktop/src/ui/time.rs desktop/src/ui/play.rs desktop/src/ui/mod.rs desktop/src/shell.rs
git commit -m "Add the desktop play view: title bar, fullscreen chrome, time control and HUD"
```

---

### Task 13: Pause menu, save slots, confirmations and the problem log

**Files:**
- Create: `desktop/src/ui/panels.rs`
- Modify: `desktop/src/ui/mod.rs`, `desktop/src/shell.rs` (call `panels::show`; handle `OpenSlots`, `SaveRequested`, `ResetRequested`, `ResetConfirmed`, `OpenProblems`)

**Interfaces:**
- Produces: `pub fn show(ctx: &egui::Context, app: &mut App)` drawing, by `app.panel`: `Pause` (the pause panel), `Slots` (the save states panel), `Problems` (the log); then `app.dialog`: `ConfirmReset`, `ConfirmReplace(n)`, `ConfirmDelete(game)`, `Message`; then `app.message` through `widgets::message_bar`.

- [ ] **Step 1: Write panels.rs**

```rust
//! The pause menu, the ten slots, the confirmations and the problem log.
//! Every string here is the Android one.

use super::theme::*;
use super::widgets;
use super::{Action, Dialog, Panel};
use crate::shell::App;
use egui::{Align2, Rect, Sense, Vec2};

pub fn show(ctx: &egui::Context, app: &mut App) {
    match app.panel {
        Panel::Pause => pause(ctx, app),
        Panel::Slots => slots(ctx, app),
        Panel::Problems => problems(ctx, app),
        _ => {}
    }
    if let Some(dialog) = app.dialog.clone() {
        confirm(ctx, app, dialog);
    }
    if let Some(text) = app.message.clone() {
        if widgets::message_bar(ctx, &text) {
            app.message = None;
        }
    }
}

fn pause(ctx: &egui::Context, app: &mut App) {
    let fullscreen = app.fullscreen;
    widgets::panel(ctx, "pause", "Take your time", Some("Progress saves automatically when you pause or leave."), false, |ui| {
        if widgets::primary(ui, "Resume game").clicked() {
            app.actions.push(Action::Resume);
        }
        ui.add_space(8.0);
        let half = (ui.available_width() - 10.0) / 2.0;
        ui.horizontal(|ui| {
            if widgets::tile(ui, "Save states", half).clicked() {
                app.actions.push(Action::OpenSlots);
            }
            if widgets::tile(ui, "Screenshot", half).clicked() {
                app.actions.push(Action::Screenshot);
            }
        });
        ui.horizontal(|ui| {
            if widgets::tile(ui, if fullscreen { "Exit full screen" } else { "Full screen" }, half).clicked() {
                app.actions.push(Action::ToggleFullscreen);
            }
            if widgets::tile(ui, "Settings", half).clicked() {
                app.actions.push(Action::OpenSettings);
            }
        });
        ui.add_space(8.0);
        if widgets::secondary(ui, "Reset game").clicked() {
            app.actions.push(Action::ResetRequested);
        }
        if widgets::secondary(ui, "Back to your shelf").clicked() {
            app.actions.push(Action::BackToShelf);
        }
    });
}

fn slots(ctx: &egui::Context, app: &mut App) {
    let Some(session) = &app.session else { return };
    let id = session.game.id.clone();
    let slots = app.library.slots(&id);
    let back = widgets::panel(ctx, "slots", "Save states", Some("Ten slots, plus a separate automatic save."), true, |ui| {
        let tile_width = 180.0;
        let columns = ((ui.available_width() + 10.0) / (tile_width + 10.0)).floor().max(1.0) as usize;
        for chunk in slots.chunks(columns) {
            ui.horizontal_top(|ui| {
                for slot in chunk {
                    egui::Frame::new().fill(SURFACE_HIGH).corner_radius(CORNER_MEDIUM).inner_margin(10.0).show(ui, |ui| {
                        ui.set_width(tile_width - 20.0);
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(tile_width - 20.0, (tile_width - 20.0) * 0.75), Sense::hover());
                        match app.slot_texture(ui.ctx(), slot) {
                            Some(texture) => ui.painter().image(texture.id(), rect, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE),
                            None => ui.painter().rect_filled(rect, CORNER_SMALL, WELL),
                        }
                        ui.label(egui::RichText::new(format!("Slot {}", slot.number + 1)).size(15.0).strong());
                        let when = match slot.time {
                            Some(ms) => chrono::DateTime::from_timestamp_millis(ms).map(|t| t.with_timezone(&chrono::Local).format("%x %R").to_string()).unwrap_or_default(),
                            None => "Empty · ready for a moment".to_string(),
                        };
                        widgets::note(ui, &when);
                        ui.horizontal(|ui| {
                            if widgets::compact(ui, "Save", true).clicked() {
                                app.actions.push(if slot.time.is_some() { Action::SaveRequested(slot.number) } else { Action::SaveConfirmed(slot.number) });
                            }
                            if widgets::compact(ui, "Load", slot.time.is_some()).clicked() {
                                app.actions.push(Action::Load(slot.number));
                            }
                        });
                    });
                }
            });
        }
        ui.add_space(8.0);
        if widgets::primary(ui, "Resume game").clicked() {
            app.actions.push(Action::Resume);
        }
    });
    if back {
        app.actions.push(Action::ClosePanel);
    }
}

fn problems(ctx: &egui::Context, app: &mut App) {
    let lines = app.library.problems();
    widgets::panel(ctx, "problems", "Problem log", None, false, |ui| {
        if lines.is_empty() {
            widgets::note(ui, "Nothing has gone wrong yet.");
        }
        for line in lines.iter().take(40) {
            ui.label(egui::RichText::new(line).size(12.0).monospace());
        }
        ui.add_space(8.0);
        if widgets::secondary(ui, "Close").clicked() {
            app.actions.push(Action::ClosePanel);
        }
    });
}

fn confirm(ctx: &egui::Context, app: &mut App, dialog: Dialog) {
    let (title, body, yes, no, danger, yes_action) = match &dialog {
        Dialog::ConfirmReset => {
            let title = app.session.as_ref().map(|s| s.game.title.clone()).unwrap_or_default();
            ("Reset game?".to_string(), format!("Restart {title} from the beginning. In-game saves and manual save slots are kept. The current session and rewind history will be replaced."), "Reset", "Cancel", false, Action::ResetConfirmed)
        }
        Dialog::ConfirmReplace(n) => (format!("Replace slot {}?", n + 1), "This replaces the progress saved in this slot. Your other slots stay available.".to_string(), "Replace save", "Keep it", false, Action::SaveConfirmed(*n)),
        Dialog::ConfirmDelete(game) => (format!("Delete {}?", game.title), "This removes the game, its battery save and all ten of its save states from this computer. It cannot be undone.".to_string(), "Delete forever", "Keep it", true, Action::DeleteConfirmed(game.clone())),
        Dialog::Message(text) => (String::new(), text.clone(), "OK", "", false, Action::CloseDialog),
    };
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("dialog-scrim")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
        ui.painter().rect_filled(rect, 0.0, SCRIM);
    });
    egui::Area::new(egui::Id::new("dialog")).order(egui::Order::Tooltip).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
        egui::Frame::new().fill(SURFACE).corner_radius(CORNER_LARGE).inner_margin(24.0).show(ui, |ui| {
            ui.set_width(420.0_f32.min(screen.width() - 48.0));
            if !title.is_empty() {
                ui.label(egui::RichText::new(&title).size(22.0).strong());
            }
            ui.label(egui::RichText::new(&body).size(15.0));
            ui.add_space(12.0);
            if widgets::quiet(ui, yes, danger).clicked() {
                app.actions.push(yes_action.clone());
            }
            if !no.is_empty() && widgets::quiet(ui, no, false).clicked() {
                app.actions.push(Action::CloseDialog);
            }
        });
    });
}
```

- [ ] **Step 2: Handle the panel actions in shell.rs**

```rust
        Action::OpenSlots => app.panel = Panel::Slots,
        Action::SaveRequested(n) => app.dialog = Some(Dialog::ConfirmReplace(n)),
        Action::ResetRequested => app.dialog = Some(Dialog::ConfirmReset),
        Action::ResetConfirmed => {
            app.dialog = None;
            if let Some(session) = &mut app.session {
                match session.reset() {
                    Ok(()) => {
                        app.panel = Panel::None;
                        app.scrub_fraction = 0.0;
                    }
                    Err(e) => {
                        app.panel = Panel::Pause;
                        let title = session.game.title.clone();
                        app.report("Game reset, but its automatic save couldn't be updated.", &format!("{title}: {e}"));
                    }
                }
                app.thumbs.clear();
            }
        }
        Action::OpenProblems => app.panel = Panel::Problems,
```

`Action::ClosePanel` from the problem log must return to Settings when it was opened from there: `app.panel_before` (already on `App`) is set when opening Problems (from Settings in Task 14) and restored on close. Replace the panels placeholder in `shell::draw` with `crate::ui::panels::show(ctx, app)`.

- [ ] **Step 3: Build, run, verify**

Run: `cargo build -p nes-desktop && cargo clippy -p nes-desktop --all-targets --no-deps -- -D warnings`, then with a game: Menu shows "Take your time"; Save states shows ten tiles; Save into slot 1 is immediate, Save again asks "Replace slot 1?"; Load restores and says "Save loaded. Press Resume when you're ready."; Screenshot writes to `~/Pictures/Emulia` and says so; Reset game asks and restarts with battery kept; Back to your shelf returns with the autosave thumbnail as the cover; Escape backs out one level at a time; Start on a controller resumes from the pause panel.

- [ ] **Step 4: Commit**

```bash
git add desktop/src/ui/panels.rs desktop/src/ui/mod.rs desktop/src/shell.rs
git commit -m "Add the desktop pause menu, save slots, confirmations and problem log"
```

---

### Task 14: Settings panel with live preview, palette import and the mapping wizard

**Files:**
- Create: `desktop/src/ui/settings.rs`
- Modify: `desktop/src/ui/mod.rs`, `desktop/src/shell.rs` (call `settings::show`; handle `OpenSettings`, `CloseSettings`, `SetLook`, `SetAspect`, `SetPalette`, `SetTrim`, `ImportPalette`, `StartWizard`, `CancelWizard`; apply palette changes to the session)

**Interfaces:**
- Produces: `pub fn show(ctx: &egui::Context, app: &mut App, video: &mut Video)`; `pub const PREVIEW_WIDTH: i32 = 640`, `PREVIEW_HEIGHT: i32 = 480`
- `App` gains `pub fn apply_palette(&mut self)` (pushes `settings.colours(&data_dir)` into the session's engine, marks `preview_dirty`)

- [ ] **Step 1: Write settings.rs**

```rust
//! Picture settings with the real pipeline previewing them, the palette
//! import, the controller wizard, and the read-only rows.

use super::theme::*;
use super::widgets;
use super::{Action, Panel};
use crate::input::NesButton;
use crate::picture::{Aspect, Look, PaletteChoice};
use crate::shell::App;
use crate::video::Video;
use egui::{Rect, Sense, Vec2};

pub const PREVIEW_WIDTH: i32 = 640;
pub const PREVIEW_HEIGHT: i32 = 480;

pub fn show(ctx: &egui::Context, app: &mut App, video: &mut Video) {
    match app.panel {
        Panel::Settings => settings(ctx, app, video),
        Panel::Mapping => mapping(ctx, app),
        _ => {}
    }
}

fn refresh_preview(ctx: &egui::Context, app: &mut App, video: &mut Video) {
    if !app.preview_dirty {
        return;
    }
    let source = app.sample_source();
    match video.preview(&source, app.settings.look, app.settings.aspect, app.settings.trim_edges, PREVIEW_WIDTH, PREVIEW_HEIGHT) {
        Ok(rgba) => {
            let image = egui::ColorImage::from_rgba_unmultiplied([PREVIEW_WIDTH as usize, PREVIEW_HEIGHT as usize], &rgba);
            match &mut app.preview {
                Some(texture) => texture.set(image, egui::TextureOptions::LINEAR),
                None => app.preview = Some(ctx.load_texture("preview", image, egui::TextureOptions::LINEAR)),
            }
            app.preview_dirty = false;
        }
        Err(e) => app.library.log_problem("Preview", &e),
    }
}

fn settings(ctx: &egui::Context, app: &mut App, video: &mut Video) {
    refresh_preview(ctx, app, video);
    let settings = app.settings.clone();
    let audio_ms = app.audio_ms;
    let has_file = crate::settings::imported_palette(&app.data_dir).is_some();
    widgets::panel(ctx, "settings", "Settings", None, false, |ui| {
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, width * 0.75), Sense::hover());
        match &app.preview {
            Some(texture) => ui.painter().image(texture.id(), rect, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE),
            None => {
                ui.painter().rect_filled(rect, CORNER_MEDIUM, WELL);
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "Preparing a preview…", egui::FontId::proportional(14.0), ON_SURFACE_VARIANT);
            }
        }

        widgets::section(ui, "Shape");
        if let Some(a) = widgets::choice_row(ui, &Aspect::ALL, settings.aspect, Aspect::label) {
            app.actions.push(Action::SetAspect(a));
        }
        widgets::note(ui, "Pixel-perfect drops to the next whole multiple");

        widgets::section(ui, "Look");
        if let Some(l) = widgets::choice_row(ui, &Look::ALL, settings.look, Look::label) {
            app.actions.push(Action::SetLook(l));
        }
        widgets::note(ui, settings.look.note());

        widgets::section(ui, "Colours");
        if let Some(p) = widgets::choice_row(ui, &PaletteChoice::ALL, settings.palette, PaletteChoice::label) {
            app.actions.push(if p == PaletteChoice::File && !has_file { Action::ImportPalette } else { Action::SetPalette(p) });
        }
        widgets::note(ui, settings.palette.note());
        if settings.palette == PaletteChoice::File && widgets::value_row(ui, "Palette file", "Load a different .pal file", "Replace").clicked() {
            app.actions.push(Action::ImportPalette);
        }

        widgets::section(ui, "Picture");
        let mut trim = settings.trim_edges;
        if widgets::switch_row(ui, "Trim the edges", "Hides the 8 rows a television lost to overscan", &mut trim) {
            app.actions.push(Action::SetTrim(trim));
        }

        widgets::section(ui, "Controls");
        if widgets::value_row(ui, "Controller buttons", "Map A, B, Select and Start for a controller or the keyboard", "Set up").clicked() {
            app.actions.push(Action::StartWizard);
        }

        widgets::section(ui, "About");
        widgets::info_row(ui, "Audio delay", &format!("{audio_ms:.1} ms"), "Sound queued ahead of the speaker");
        widgets::info_row(ui, "Version", env!("CARGO_PKG_VERSION"), "Emulia, on this computer");
        if widgets::value_row(ui, "Problem log", "What went wrong, and why", "Open").clicked() {
            app.actions.push(Action::OpenProblems);
        }
        ui.add_space(12.0);
        if widgets::primary(ui, "Done").clicked() {
            app.actions.push(Action::CloseSettings);
        }
    });
}

fn mapping(ctx: &egui::Context, app: &mut App) {
    let Some(wizard) = &app.wizard else { return };
    let step = wizard.step();
    let device = wizard.device_name().map(str::to_string);
    widgets::panel(ctx, "mapping", "Controller buttons", Some("Press each button on the controller or keyboard you want to use."), false, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(12.0);
            ui.label(egui::RichText::new(format!("Press  {}", NesButton::ORDER[step.min(3)].label())).size(44.0).strong().color(LEAF));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                for (i, b) in NesButton::ORDER.iter().enumerate() {
                    let text = if i < step { format!("✓ {}", b.label()) } else { b.label().to_string() };
                    ui.label(egui::RichText::new(text).size(15.0).color(if i < step { LEAF } else { ON_SURFACE_VARIANT }));
                }
            });
            widgets::note(ui, &format!("Step {} of 4", step + 1));
            if let Some(name) = device {
                widgets::note(ui, &name);
            }
        });
        ui.add_space(16.0);
        if widgets::secondary(ui, "Cancel").clicked() {
            app.actions.push(Action::CancelWizard);
        }
    });
}
```

- [ ] **Step 2: Handle the settings actions in shell.rs**

```rust
        Action::OpenSettings => {
            app.panel_before = app.panel;
            app.panel = Panel::Settings;
            app.preview_dirty = true;
        }
        Action::CloseSettings => {
            app.panel = if app.session.is_some() { Panel::Pause } else { Panel::None };
            app.preview = None;
        }
        Action::SetLook(look) => { app.settings.look = look; app.settings_dirty = true; app.preview_dirty = true; }
        Action::SetAspect(aspect) => { app.settings.aspect = aspect; app.settings_dirty = true; app.preview_dirty = true; }
        Action::SetTrim(trim) => { app.settings.trim_edges = trim; app.settings_dirty = true; app.preview_dirty = true; }
        Action::SetPalette(palette) => {
            app.settings.palette = palette;
            app.settings_dirty = true;
            app.apply_palette();
        }
        Action::ImportPalette => {
            if let Some(path) = rfd::FileDialog::new().add_filter("Palette", &["pal"]).set_title("Choose a palette").pick_file() {
                match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| crate::settings::store_palette(&app.data_dir, &b)) {
                    Ok(_) => {
                        app.settings.palette = crate::picture::PaletteChoice::File;
                        app.settings_dirty = true;
                        app.apply_palette();
                        app.message = Some("Palette loaded.".into());
                    }
                    Err(e) => app.report("That palette file didn't work.", &e),
                }
            }
        }
        Action::StartWizard => {
            app.wizard = Some(Wizard::new());
            app.panel = Panel::Mapping;
            app.input.clear();
        }
        Action::CancelWizard => {
            app.wizard = None;
            app.panel = if app.session.is_some() { Panel::Pause } else { Panel::None };
        }
```

And on `App`:

```rust
    pub fn apply_palette(&mut self) {
        let colours = self.settings.colours(&self.data_dir);
        if let Some(session) = &mut self.session {
            session.engine.set_palette(colours);
        }
        self.preview_dirty = true;
    }
```

Call `app.apply_palette()` right after `Session::open` succeeds in `open_game` so a game opens with the chosen colours. In `finish_wizard` (Task 9) the panel returns to `Panel::Pause` when a session exists, else `Panel::None`. `Action::OpenProblems` from Settings sets `panel_before = Panel::Settings` so `ClosePanel` returns there; the existing `ClosePanel` arm becomes `app.panel = std::mem::replace(&mut app.panel_before, Panel::None)` when the current panel is `Problems`.

Replace the settings placeholder in `shell::draw` with `crate::ui::settings::show(ctx, app, video)`.

- [ ] **Step 3: Build, run, verify**

Run: `cargo build -p nes-desktop && cargo clippy -p nes-desktop --all-targets --no-deps -- -D warnings`. Then:
- From the shelf, Settings shows the preview from the newest thumbnail (or the built pattern), each Look changes it through the real shader, Shape and Trim change the letterbox, Colours restains the pattern, From a file opens the picker and a valid `.pal` shows "Palette loaded." with a Replace row; a short file shows "That palette file didn't work." and the log has the reason.
- From a game, the preview is the paused frame; Done returns to the pause menu; the game shows the chosen look and palette immediately.
- Set up: press four keys on the keyboard; "Buttons saved for Keyboard…" appears; the new keys play; a controller mapped the same way is saved under its GUID in `controllers.json`; pressing the same button twice is ignored; Escape cancels.
- Audio delay shows a number around 30 ms while a game plays.

- [ ] **Step 4: Commit**

```bash
git add desktop/src/ui/settings.rs desktop/src/ui/mod.rs desktop/src/shell.rs
git commit -m "Add desktop settings with live preview, palette import and the mapping wizard"
```

---

### Task 15: Documentation, CI and final verification

**Files:**
- Modify: `docs/DESKTOP.md`, `README.md`, `.github/workflows/desktop.yml` (only if Task 11 chose a Linux dialog backend that needs a package)

- [ ] **Step 1: Rewrite docs/DESKTOP.md**

Keep the Build section as it is (add any Linux dialog dependency from Task 11). Replace the introduction's last two sentences with what the shell now has, and replace "Play and saves" with three sections:

**Shelf and play**: no arguments opens the shelf; a ROM path or a dropped `.nes` file is added and opened; box art by menu or by dropping an image; put away, bring back, delete; Menu / Escape / Space open the pause menu; F11 full screen with the chrome fading after five seconds.

**Keys and controllers**: the table below.

| Key | Action |
|---|---|
| Arrow keys | Direction pad |
| X / Z | A / B |
| Enter / Right Shift | Start / Select |
| Escape or Space | Pause menu (Escape also backs out of panels) |
| F11 | Full screen |
| F5 / F8 | Save / load slot 1 |
| . / , (hold) | Fast-forward / rewind 2×, Shift for 6× |
| Backspace | Jump back 5 s, Shift for 15 s |

Controllers: any SDL game controller; south = B, east = A, Back = Select, Start = Start; shoulders 2×, triggers 6×; two ports in connection order; Settings → Controller buttons → Set up remaps A, B, Select and Start per controller or for the keyboard.

**Saves and data**: the layout from the spec, the migration of `<identity>.sav` / `.state`, screenshots in `~/Pictures/Emulia`, `--data-dir` and the `--save-dir` alias, battery flush timing, and that Android save containers are not imported.

Update Verification: `--frames` is headless; the smoke test checks the library import too; `python3 scripts/check-shaders.py` covers the desktop GLSL; list the manual checks from Tasks 9 to 14 as a checklist.

- [ ] **Step 2: Update README.md**

Change the desktop line under the introduction to describe the shell, and the `desktop/` row in Layout to `desktop/    nes-desktop: macOS/Linux SDL2 + OpenGL + egui shell: shelf, saves, time control, looks, gamepads`.

- [ ] **Step 3: Run everything**

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --locked -p nes-core -p nes-runner -p nes-desktop --all-targets --no-deps -- -D warnings
cargo build --release --locked -p nes-desktop -p nes-runner
python3 scripts/check-desktop.py target/release/nes-desktop --require-native-drivers
python3 scripts/check-shaders.py
```

Expected: all green. Then the manual checklist from `docs/DESKTOP.md` with a ROM you own, including one controller.

- [ ] **Step 4: Commit**

```bash
git add docs/DESKTOP.md README.md .github/workflows/desktop.yml
git commit -m "Document the desktop shell and its verification"
```

---

## Self-review notes

- Spec coverage: shelf (Task 11), play view and time control (12), pause menu, slots, reset, screenshot, problem log (13), settings, looks, palettes, preview, wizard (14), input and profiles (7), data layout and migration (4, 5), CLI and headless (6), GL pipeline (8), theme (10), tests (every task), docs and CI (15). Autosave triggers: pause (Task 9 `Action::Pause`), focus loss (Task 9 event loop pushes `Pause`), leaving (`BackToShelf` calls `close`), quit (end of `run`), reset (`Session::reset`).
- Names used across tasks: `Session::{open, advance, take_error, flush_battery, save, load, reset, record_playtime, screenshot, close, audio_ms, until_due}`, `Engine::{new, id, identity, frame_rate, frames_for, set_palette, frame, step, samples, rewind_step, rewind_depth, save_state, load_state, battery_ram, load_battery, reset, header_notes}`, `Library::{open, from_root, root, data_dir, games, archived, find, add, set_archived, forget, record, directory, rom_path, battery_path, state_path, thumbnail_path, art_path, slots, has_autosave, cover, set_art, clear_art, log_problem, problems}`, `Input::{new, profiles, set_profile, take_dirty, key, clear, pad_added, pad_removed, pad, pad_button, pad_axis, buttons, time_speed, jump_back, is_start}`, `Video::{new, gl, upload, draw, preview, destroy}`, `Bridge::{new, modifiers, handle, pixels_per_point, begin, finish, paint, destroy}`, `App::{report, notice, playing, cover_texture, slot_texture, sample_source, apply_palette}`.
