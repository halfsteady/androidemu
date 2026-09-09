mod palette;

use nes_core::{Buttons, Nes};
use sdl2::{
    audio::AudioSpecDesired,
    event::{Event, WindowEvent},
    keyboard::{Keycode, Scancode},
    pixels::PixelFormatEnum,
};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, Instant},
};

const HELP: &str = "Emulia desktop
Usage: nes-desktop <rom.nes> [--mute] [--save-dir <directory>] [--frames <count>]
Arrows: move   Z: B   X: A   Enter: Start   Right Shift: Select
Space: pause   R: reset   F5: save state   F8: load state   Escape: quit
Battery saves are written every 5 seconds and on clean exit.
--frames runs a finite number of frames (also useful with SDL dummy drivers).";

struct Options {
    rom: PathBuf,
    save_dir: PathBuf,
    mute: bool,
    frames: Option<u64>,
}

fn default_save_dir() -> Result<PathBuf, String> {
    if cfg!(target_os = "macos") {
        return std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library/Application Support/Emulia"))
            .ok_or("HOME is unset; use --save-dir".into());
    }
    if let Some(path) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return Ok(path.join("emulia"));
    }
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".local/share/emulia"))
        .ok_or("HOME is unset; use --save-dir".into())
}

fn options(args: impl Iterator<Item = OsString>) -> Result<Option<Options>, String> {
    let mut args = args.peekable();
    if args.peek().is_none() {
        println!("{HELP}");
        return Ok(None);
    }
    let (mut rom, mut save_dir, mut mute, mut frames) = (None, None, false, None);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => {
                println!("{HELP}");
                return Ok(None);
            }
            Some("--mute") => mute = true,
            Some("--save-dir") => {
                save_dir = Some(PathBuf::from(
                    args.next().ok_or("--save-dir needs a directory")?,
                ))
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
        rom: rom.ok_or("Provide a ROM path")?,
        save_dir: match save_dir {
            Some(p) => p,
            None => default_save_dir()?,
        },
        mute,
        frames,
    }))
}

// Write in the same directory so rename atomically replaces the old save on
// macOS/Linux. create_new avoids accidentally following an existing temp file.
fn write_save(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
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

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn play(nes: &mut Nes, options: &Options, battery: &Path, state: &Path) -> Result<(), String> {
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Emulia", 768, 720)
        .position_centered()
        .resizable()
        .allow_highdpi()
        .build()
        .map_err(|e| e.to_string())?;
    // No vsync: the audio device clocks emulation, including PAL's 50 Hz.
    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    canvas
        .set_logical_size(256, 240)
        .map_err(|e| e.to_string())?;
    let textures = canvas.texture_creator();
    let mut texture = textures
        .create_texture_streaming(PixelFormatEnum::RGB24, 256, 240)
        .map_err(|e| e.to_string())?;
    let audio = if options.mute {
        None
    } else {
        let subsystem = sdl.audio()?;
        Some(
            subsystem
                .open_queue::<f32, _>(
                    None,
                    &AudioSpecDesired {
                        freq: Some(48_000),
                        channels: Some(1),
                        samples: Some(512),
                    },
                )
                .map_err(|e| {
                    format!("Cannot open audio: {e}. Use --mute to play without sound.")
                })?,
        )
    };
    let mut events = sdl.event_pump()?;
    let frame_time = Duration::from_secs_f64(1.0 / nes.bus.cart.header.region.frame_rate());
    let target_bytes = (48_000.0 * frame_time.as_secs_f64() * 2.0).ceil() as u32 * 4;
    let mut deadline = Instant::now();
    let mut last_save = Instant::now();
    let (mut paused, mut unfocused, mut frames) = (false, false, 0_u64);
    loop {
        for event in events.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => return Ok(()),
                Event::Window {
                    win_event: WindowEvent::FocusLost,
                    ..
                } => unfocused = true,
                Event::Window {
                    win_event: WindowEvent::FocusGained,
                    ..
                } => unfocused = false,
                Event::KeyDown {
                    keycode: Some(key),
                    repeat: false,
                    ..
                } => match key {
                    Keycode::Space => paused = !paused,
                    Keycode::R => {
                        nes.reset();
                        if let Some(a) = &audio {
                            a.clear();
                        }
                    }
                    Keycode::F5 => {
                        if let Err(e) = write_save(state, &nes.save_state()) {
                            eprintln!("{e}");
                        }
                    }
                    Keycode::F8 => {
                        match fs::read(state)
                            .map_err(|e| e.to_string())
                            .and_then(|b| nes.load_state(&b).map_err(|e| e.to_string()))
                        {
                            Ok(()) => {
                                if let Some(a) = &audio {
                                    a.clear();
                                }
                            }
                            Err(e) => eprintln!("Cannot load {}: {e}", state.display()),
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        if paused || unfocused {
            if let Some(a) = &audio {
                a.pause();
                a.clear();
            }
            nes.set_buttons(0, Buttons(0));
            deadline = Instant::now();
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }
        // Keep polling input while waiting; never grow the sound queue unbounded.
        if let Some(a) = &audio {
            if a.size() >= target_bytes {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
        } else if Instant::now() < deadline {
            std::thread::sleep(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(1)),
            );
            continue;
        }
        let keyboard = events.keyboard_state();
        let mut buttons = 0;
        for (key, bit) in [
            (Scancode::X, Buttons::A),
            (Scancode::Z, Buttons::B),
            (Scancode::Return, Buttons::START),
            (Scancode::RShift, Buttons::SELECT),
            (Scancode::Up, Buttons::UP),
            (Scancode::Down, Buttons::DOWN),
            (Scancode::Left, Buttons::LEFT),
            (Scancode::Right, Buttons::RIGHT),
        ] {
            if keyboard.is_scancode_pressed(key) {
                buttons |= bit;
            }
        }
        nes.set_buttons(0, Buttons(buttons));
        nes.step_frame();
        if let Some(a) = &audio {
            a.queue_audio(nes.bus.apu.samples())?;
            a.resume();
        }
        texture.with_lock(None, |pixels, pitch| {
            for (i, &index) in nes.framebuffer().iter().enumerate() {
                let rgb = palette::PALETTE[(index & 63) as usize];
                let offset = (i / 256) * pitch + (i % 256) * 3;
                pixels[offset..offset + 3].copy_from_slice(&[
                    (rgb >> 16) as u8,
                    (rgb >> 8) as u8,
                    rgb as u8,
                ]);
            }
        })?;
        canvas.clear();
        canvas.copy(&texture, None, None)?;
        canvas.present();
        frames += 1;
        if options.frames.is_some_and(|limit| frames >= limit) {
            return Ok(());
        }
        deadline += frame_time;
        if Instant::now().saturating_duration_since(deadline) > frame_time {
            deadline = Instant::now();
        }
        if last_save.elapsed() >= Duration::from_secs(5) {
            if let Some(bytes) = nes.battery_ram() {
                write_save(battery, bytes)?;
            }
            last_save = Instant::now();
        }
    }
}

fn run() -> Result<(), String> {
    let Some(options) = options(std::env::args_os().skip(1))? else {
        return Ok(());
    };
    let rom = fs::read(&options.rom).map_err(|e| format!("{}: {e}", options.rom.display()))?;
    let mut nes = Nes::new(&rom).map_err(|e| e.to_string())?;
    fs::create_dir_all(&options.save_dir)
        .map_err(|e| format!("{}: {e}", options.save_dir.display()))?;
    let id = format!("{:016x}", nes.bus.cart.header.identity);
    let battery = options.save_dir.join(format!("{id}.sav"));
    let state = options.save_dir.join(format!("{id}.state"));
    if nes.battery_ram().is_some() {
        if let Some(bytes) = read_optional(&battery)? {
            nes.load_battery_ram(&bytes)
                .map_err(|e| format!("{}: {e}", battery.display()))?;
        }
    }
    println!("{HELP}\nSaves: {}", options.save_dir.display());
    let result = play(&mut nes, &options, &battery, &state);
    if let Some(bytes) = nes.battery_ram() {
        write_save(&battery, bytes)?;
    }
    result
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_save_replaces_and_preserves_previous_file_on_collision() {
        let dir = std::env::temp_dir().join(format!("emulia-save-test-{}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("game.sav");
        write_save(&path, b"old").unwrap();
        write_save(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        let temp = path.with_extension(format!("tmp-{}", std::process::id()));
        fs::write(&temp, b"existing temporary file").unwrap();
        assert!(write_save(&path, b"replacement").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(fs::read(&temp).unwrap(), b"existing temporary file");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn file_save_state_restores_the_machine_and_replays() {
        let mut rom = vec![0; 16 + 16384];
        rom[..4].copy_from_slice(b"NES\x1a");
        rom[4] = 1;
        rom[16..22].copy_from_slice(&[0xe6, 0x00, 0x4c, 0x00, 0x80, 0xea]);
        rom[16 + 0x3ffc..16 + 0x3ffe].copy_from_slice(&[0, 0x80]);
        let mut nes = Nes::new(&rom).unwrap();
        nes.step_frame();
        let dir = std::env::temp_dir().join(format!("emulia-state-test-{}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("game.state");
        write_save(&path, &nes.save_state()).unwrap();
        nes.step_frame();
        let expected = nes.save_state();
        nes.load_state(&fs::read(&path).unwrap()).unwrap();
        nes.step_frame();
        assert_eq!(nes.save_state(), expected);
        fs::remove_dir_all(dir).unwrap();
    }
}
