#[allow(dead_code)] // Rewind, states and palettes gain their callers in Task 9.
mod engine;
#[allow(dead_code)] // pictures_dir and read_png wait for screenshots and art in Task 9.
mod files;
#[allow(dead_code)] // Pads, profiles and the wizard gain their callers in Task 9.
mod input;
#[allow(dead_code)] // Playtime and thumbnails are for the shelf screen in Task 9.
mod library;
mod palette;
#[allow(dead_code)] // The geometry, looks and palettes gain their callers in Task 9.
mod picture;
#[allow(dead_code)] // The scrubber gains its caller with the play screen in Task 9.
mod scrub;
#[allow(dead_code)] // Slots, screenshots and playtime gain their callers in Task 9.
mod session;
#[allow(dead_code)] // Settings and controller profiles gain their callers in Task 9.
mod settings;
mod shell;

use library::{Game, Library};
use session::{Audio, Session};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Duration,
};

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

fn options(mut args: impl Iterator<Item = OsString>) -> Result<Option<Options>, String> {
    let (mut rom, mut data_dir, mut mute, mut frames) = (None, None, false, None);
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
            Some("--data-dir" | "--save-dir") => {
                data_dir = Some(PathBuf::from(
                    args.next().ok_or("--data-dir needs a directory")?,
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
            &sdl2::audio::AudioSpecDesired {
                freq: Some(48_000),
                channels: Some(1),
                samples: Some(512),
            },
        )
        .map_err(|e| format!("Cannot open audio: {e}. Use --mute to play without sound."))?;
    Ok(Box::new(queue))
}

/// The engine, the saves and the audio queue with no window: what CI can run
/// under SDL's dummy drivers, and what `--frames` means.
fn headless(options: &Options, frames: u64) -> Result<(), String> {
    let rom = options.rom.as_ref().ok_or("--frames needs a ROM path")?;
    std::fs::create_dir_all(&options.data_dir)
        .map_err(|e| format!("{}: {e}", options.data_dir.display()))?;
    let library = Library::open(&options.data_dir)?;
    let game = import(&library, rom)?;
    let sdl = sdl2::init()?;
    let audio = if options.mute {
        None
    } else {
        Some(open_audio(&sdl)?)
    };
    let (mut session, warnings) = Session::open(&library, game, audio)?;
    for warning in &warnings {
        eprintln!("{warning}");
    }
    // Without a window there is nobody to decide whether to carry on from an
    // earlier point, so a run that lost progress has to fail rather than save
    // over what is left.
    if !warnings.is_empty() {
        return Err(warnings.join(" "));
    }
    session.paused = false;
    println!("{HELP}\nData: {}", options.data_dir.display());
    let mut done = 0;
    while done < frames {
        done += session
            .advance(nes_core::Buttons(0), nes_core::Buttons(0))
            .frames as u64;
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
        let o = parse(&[
            "game.nes",
            "--mute",
            "--data-dir",
            "/tmp/x",
            "--frames",
            "6",
        ])
        .unwrap()
        .unwrap();
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
        assert!(
            headless(&parse(&["--frames", "5", "--mute"]).unwrap().unwrap(), 5)
                .unwrap_err()
                .contains("ROM")
        );
    }

    #[test]
    fn help_and_list_drivers_print_and_exit() {
        assert!(parse(&["--help"]).unwrap().is_none());
    }
}
