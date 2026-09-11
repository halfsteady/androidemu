#[allow(dead_code)] // The machine and its rewind chain gain their callers in Task 9.
mod engine;
#[allow(dead_code)] // pictures_dir alone is still waiting; screenshots arrive in Task 9.
mod files;
#[allow(dead_code)] // The shelf, its folders and the problem log gain callers in Task 9.
mod library;
mod palette;
#[allow(dead_code)] // The geometry, looks and palettes gain their callers in Task 9.
mod picture;
#[allow(dead_code)] // The scrubber gains its caller with the play screen in Task 9.
mod scrub;
#[allow(dead_code)] // One open game and everything it saves gain callers in Task 9.
mod session;
#[allow(dead_code)] // Settings and controller profiles gain their callers in Task 9.
mod settings;

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
Battery saves are written every 5 seconds, when pausing and on clean exit.
--frames runs a finite number of frames (also useful with SDL dummy drivers).
--list-drivers lists the compiled SDL video and audio backends.";

struct Options {
    rom: PathBuf,
    save_dir: PathBuf,
    mute: bool,
    frames: Option<u64>,
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
            Some("--list-drivers") => {
                println!(
                    "Video: {}",
                    sdl2::video::drivers().collect::<Vec<_>>().join(", ")
                );
                println!(
                    "Audio: {}",
                    sdl2::audio::drivers().collect::<Vec<_>>().join(", ")
                );
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
            None => files::default_data_dir()?,
        },
        mute,
        frames,
    }))
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
    let mut was_paused = false;
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
                        if let Err(e) = files::write_atomic(state, &nes.save_state()) {
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
        let is_paused = paused || unfocused;
        // Flush when putting a game aside. Otherwise a focus pause can leave
        // recent progress unsaved indefinitely, even beyond the save interval.
        if (is_paused && !was_paused) || last_save.elapsed() >= Duration::from_secs(5) {
            if let Some(bytes) = nes.battery_ram() {
                if let Err(e) = files::write_atomic(battery, bytes) {
                    // A temporarily unavailable save directory must not close
                    // the game and discard the only remaining copy in RAM.
                    eprintln!("{e}");
                }
            }
            last_save = Instant::now();
        }
        was_paused = is_paused;
        if is_paused {
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
        if let Some(bytes) = files::read_optional(&battery)? {
            nes.load_battery_ram(&bytes)
                .map_err(|e| format!("{}: {e}", battery.display()))?;
        }
    }
    println!("{HELP}\nSaves: {}", options.save_dir.display());
    let result = play(&mut nes, &options, &battery, &state);
    if let Some(bytes) = nes.battery_ram() {
        files::write_atomic(&battery, bytes)?;
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
    fn focus_pause_flushes_battery_without_waiting_for_resume_or_exit() {
        use nes_core::cpu::Bus;

        std::env::set_var("SDL_VIDEODRIVER", "dummy");
        std::env::set_var("SDL_AUDIODRIVER", "dummy");
        std::env::set_var("SDL_RENDER_DRIVER", "software");
        let sdl = sdl2::init().unwrap();
        let events = sdl.event().unwrap();
        let dir = std::env::temp_dir().join(format!("emulia-focus-test-{}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let battery = dir.join("game.sav");
        let mut rom = vec![0; 16 + 16384];
        rom[..4].copy_from_slice(b"NES\x1a");
        rom[4] = 1;
        rom[6] = 2;
        let mut nes = Nes::new(&rom).unwrap();
        nes.bus.write(0x6000, 0x5a);
        let options = Options {
            rom: PathBuf::new(),
            save_dir: dir.clone(),
            mute: true,
            frames: None,
        };
        events
            .push_event(Event::Window {
                timestamp: 0,
                window_id: 0,
                win_event: WindowEvent::FocusLost,
            })
            .unwrap();
        let sender = events.event_sender();
        let saved = battery.clone();
        let waiter = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            while !saved.exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            sender.push_event(Event::Quit { timestamp: 0 }).unwrap();
        });
        let result = play(&mut nes, &options, &battery, &dir.join("game.state"));
        waiter.join().unwrap();
        result.unwrap();
        // play() deliberately excludes run()'s final save: this must already
        // be on disk while the window is paused.
        assert_eq!(fs::read(&battery).unwrap()[0], 0x5a);
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
        files::write_atomic(&path, &nes.save_state()).unwrap();
        nes.step_frame();
        let expected = nes.save_state();
        nes.load_state(&fs::read(&path).unwrap()).unwrap();
        nes.step_frame();
        assert_eq!(nes.save_state(), expected);
        fs::remove_dir_all(dir).unwrap();
    }
}
