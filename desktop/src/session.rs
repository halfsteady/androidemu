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
    fn queued_bytes(&self) -> u32 {
        self.size()
    }
    fn queue(&mut self, samples: &[f32]) -> Result<(), String> {
        self.queue_audio(samples)
    }
    fn clear(&mut self) {
        sdl2::audio::AudioQueue::clear(self)
    }
    fn pause(&mut self) {
        sdl2::audio::AudioQueue::pause(self)
    }
    fn resume(&mut self) {
        sdl2::audio::AudioQueue::resume(self)
    }
    fn queued_ms(&self) -> f32 {
        self.size() as f32 / 4.0 / 48.0
    }
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
    /// Set when a battery save that would not load could not be moved aside
    /// either. The only copy of it is still on disk, so this session writes no
    /// battery at all rather than replacing it with empty RAM.
    battery_unreadable: bool,
    /// The autosave could not be restored or preserved elsewhere. A retry
    /// must keep it aside before replacing it when the directory recovers.
    auto_unreadable: bool,
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
    pub fn open(
        library: &Library,
        game: Game,
        audio: Option<Box<dyn Audio>>,
    ) -> Result<(Session, Vec<String>), String> {
        let rom = fs::read(library.rom_path(&game.id))
            .map_err(|e| format!("{}: {e}", library.rom_path(&game.id).display()))?;
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
        let mut battery_unreadable = false;
        let mut loaded_battery = None;
        if engine.battery_ram().is_some() {
            let failure = match read_optional(&battery) {
                Ok(Some(bytes)) => match engine.load_battery(&bytes) {
                    Ok(()) => {
                        loaded_battery = Some(bytes);
                        None
                    }
                    Err(e) => Some(format!("{}: {e}", battery.display())),
                },
                Ok(None) => None,
                Err(e) => Some(e),
            };
            if let Some(detail) = failure {
                battery_unreadable = !keep_save(library, &game, &battery, &detail);
                warnings.push(START_EARLIER.to_string());
                paused = true;
            }
        }
        let auto = library.state_path(&game.id, Slot::Auto);
        let boot_state = engine.save_state();
        let mut auto_unreadable = false;
        let failure = match read_optional(&auto) {
            Ok(Some(bytes)) => match engine.load_state(&bytes) {
                Ok(())
                    if loaded_battery
                        .as_deref()
                        .is_some_and(|battery| engine.battery_ram() != Some(battery)) =>
                {
                    // Battery is committed before a state. A different battery
                    // therefore belongs to later play (or an interrupted save).
                    // Keep the old state recoverable and boot with the battery;
                    // mixing that RAM into the old running machine is unsafe.
                    engine.load_state(&boot_state)?;
                    Some(format!(
                        "{} conflicts with the saved battery; starting from battery RAM",
                        auto.display()
                    ))
                }
                Ok(()) => None,
                Err(e) => Some(format!("{}: {e}", auto.display())),
            },
            Ok(None) => None,
            Err(e) => Some(e),
        };
        if let Some(detail) = failure {
            auto_unreadable = !keep_save(library, &game, &auto, &detail);
            if warnings.is_empty() {
                warnings.push(START_EARLIER.to_string());
            }
            paused = true;
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
                battery_unreadable,
                auto_unreadable,
                library_root: library.root().to_path_buf(),
                audio,
                target_bytes,
                frame_time,
                deadline: now,
                last_flush: now,
                // A game that opened paused has not been put aside by anyone,
                // so the first tick must not read as a pause transition and
                // flush a battery nobody has played yet.
                was_paused: paused,
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
        let mut result = Advance {
            frames: 0,
            hit_end: false,
        };
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
                    if let Err(e) = a.queue(self.engine.samples()) {
                        self.error = Some(e);
                    }
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

    /// How long one frame of this ROM's region lasts. What the shell paces on
    /// when the driver gave it no vsync to pace on.
    pub fn frame_time(&self) -> Duration {
        self.frame_time
    }

    /// How long until the next frame is due when there is no audio device to
    /// wait on; the headless loop sleeps this.
    pub fn until_due(&self) -> Duration {
        if self.audio.is_some() || self.paused || self.scrub != 0 {
            Duration::from_millis(1)
        } else {
            self.deadline
                .saturating_duration_since(Instant::now())
                .min(self.frame_time)
        }
    }

    pub fn flush_battery(&mut self) -> Result<(), String> {
        if self.battery_unreadable {
            return Ok(());
        }
        if let Some(bytes) = self.engine.battery_ram() {
            write_atomic(&self.battery, bytes)?;
        }
        Ok(())
    }

    /// The state, the battery RAM if any and a thumbnail, each atomically.
    pub fn save(&mut self, slot: Slot) -> Result<(), String> {
        let library = self.library();
        // A committed state must never be newer than the battery it contains.
        // Also try the battery even when the autosave path cannot be written.
        self.flush_battery()?;
        if slot == Slot::Auto && self.auto_unreadable {
            let path = library.state_path(&self.game.id, slot);
            if !keep_save(
                &library,
                &self.game,
                &path,
                "Autosave could not be restored",
            ) {
                return Err(format!(
                    "{} is protected until it can be kept aside",
                    path.display()
                ));
            }
            self.auto_unreadable = false;
        }
        write_atomic(
            &library.state_path(&self.game.id, slot),
            &self.engine.save_state(),
        )?;
        write_png(
            &library.thumbnail_path(&self.game.id, slot),
            WIDTH as u32,
            HEIGHT as u32,
            self.engine.frame(),
        )
    }

    pub fn load(&mut self, slot: Slot) -> Result<(), String> {
        let path = self.library().state_path(&self.game.id, slot);
        let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        self.engine
            .load_state(&bytes)
            .map_err(|e| format!("{}: {e}", path.display()))?;
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
            .map(|c| {
                if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let stamp = chrono::Local::now().format("%Y-%m-%d %H.%M.%S");
        let path = folder.join(format!("{title} {stamp}.png"));
        write_png(&path, WIDTH as u32, HEIGHT as u32, self.engine.frame())?;
        Ok(path)
    }

    /// The caller retains this session if persistence fails, so it can retry.
    pub fn close(&mut self) -> Result<(), String> {
        self.save(Slot::Auto)?;
        self.record_playtime();
        Ok(())
    }
}

/// Preserve every recovery copy, including files an older reader cannot load.
fn kept_aside(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".unreadable");
    let first = PathBuf::from(&name);
    let mut kept = first.clone();
    let mut number = 0;
    while kept.symlink_metadata().is_ok() {
        number += 1;
        let mut next = first.as_os_str().to_os_string();
        next.push(format!("-{number}"));
        kept = PathBuf::from(next);
    }
    kept
}

/// False means the original still needs protection from every later write.
fn keep_save(library: &Library, game: &Game, path: &Path, detail: &str) -> bool {
    let kept = kept_aside(path);
    match fs::rename(path, &kept) {
        Ok(()) => {
            library.log_problem(
                &game.title,
                &format!("{detail}; kept as {}", kept.display()),
            );
            true
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
        Err(e) => {
            library.log_problem(
                &game.title,
                &format!("{detail}; cannot be moved to {}: {e}", kept.display()),
            );
            false
        }
    }
}

/// The first desktop release keyed `<identity>.sav` and `<identity>.state` at
/// the data directory root. Move them into the game's folder once.
pub fn migrate_flat_saves(
    data_dir: &Path,
    library: &Library,
    id: &str,
    identity: &str,
) -> Result<Vec<String>, String> {
    let mut moved = Vec::new();
    for (old, new) in [
        (
            data_dir.join(format!("{identity}.sav")),
            library.battery_path(id),
        ),
        (
            data_dir.join(format!("{identity}.state")),
            library.state_path(id, Slot::Number(0)),
        ),
    ] {
        if old.exists() && !new.exists() {
            fs::rename(&old, &new).map_err(|e| format!("{}: {e}", old.display()))?;
            moved.push(format!("moved {} to {}", old.display(), new.display()));
        }
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::test_rom;
    use std::{cell::RefCell, fs, rc::Rc};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulia-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// What an audio device was asked to do. Shared with the test, because the
    /// session owns the device once it is handed over.
    #[derive(Default)]
    struct AudioRecord {
        queued: u32,
        cleared: usize,
        paused: bool,
    }

    /// Records what an audio device would have been asked to do.
    #[derive(Default, Clone)]
    struct FakeAudio(Rc<RefCell<AudioRecord>>);
    impl Audio for FakeAudio {
        fn queued_bytes(&self) -> u32 {
            self.0.borrow().queued
        }
        fn queue(&mut self, samples: &[f32]) -> Result<(), String> {
            self.0.borrow_mut().queued += samples.len() as u32 * 4;
            Ok(())
        }
        fn clear(&mut self) {
            let mut record = self.0.borrow_mut();
            record.queued = 0;
            record.cleared += 1;
        }
        fn pause(&mut self) {
            self.0.borrow_mut().paused = true;
        }
        fn resume(&mut self) {
            self.0.borrow_mut().paused = false;
        }
        fn queued_ms(&self) -> f32 {
            self.0.borrow().queued as f32 / 4.0 / 48.0
        }
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
        session.close().unwrap();
        assert!(library.has_autosave(&game.id));
        assert_eq!(fs::read(library.battery_path(&game.id)).unwrap()[0], 0x5a);
        let (session, warnings) = Session::open(&library, game.clone(), None).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(session.engine.battery_ram().unwrap()[0], 0x5a);
        fs::write(library.state_path(&game.id, Slot::Auto), b"corrupt").unwrap();
        let (session, warnings) = Session::open(&library, game.clone(), None).unwrap();
        assert_eq!(
            warnings,
            vec!["This game had to start from an earlier point.".to_string()]
        );
        assert!(session.paused);
        assert!(library.problems()[0].contains("auto.state"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn advance_runs_frames_until_the_audio_queue_holds_two_frames() {
        let dir = temp_dir("session-audio");
        let (library, game) = library_with_game(&dir);
        let (mut session, _) =
            Session::open(&library, game, Some(Box::new(FakeAudio::default()))).unwrap();
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
        let audio = FakeAudio::default();
        let record = audio.0.clone();
        let (mut session, _) =
            Session::open(&library, game.clone(), Some(Box::new(audio))).unwrap();
        use nes_core::cpu::Bus;
        session.engine.nes.bus.write(0x6000, 0x5a);
        session.paused = true;
        let paused = session.advance(Buttons(0), Buttons(0));
        assert_eq!(paused.frames, 0);
        assert_eq!(fs::read(library.battery_path(&game.id)).unwrap()[0], 0x5a);
        assert!(session.take_error().is_none());
        assert!(record.borrow().paused, "the device was left playing");
        assert!(record.borrow().cleared >= 1, "the queue was left to drain");
        assert_eq!(record.borrow().queued, 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_unreadable_battery_is_kept_aside_and_never_written_over() {
        let dir = temp_dir("session-battery");
        let (library, game) = library_with_game(&dir);
        let battery = library.battery_path(&game.id);
        fs::write(&battery, b"not a battery save").unwrap();
        let (mut session, warnings) = Session::open(&library, game.clone(), None).unwrap();
        assert_eq!(warnings, vec![START_EARLIER.to_string()]);
        assert!(session.paused);
        let kept = PathBuf::from(format!("{}.unreadable", battery.display()));
        assert_eq!(fs::read(&kept).unwrap(), b"not a battery save");
        assert!(
            !battery.exists(),
            "the empty machine must not claim the name"
        );
        assert!(library.problems()[0].contains("unreadable"));
        // A paused tick, then ordinary play, then closing: none of them may
        // reach past the file that was moved out of the way.
        session.advance(Buttons(0), Buttons(0));
        session.paused = false;
        session.advance(Buttons(0), Buttons(0));
        session.advance(Buttons(0), Buttons(0));
        session.close().unwrap();
        assert_eq!(fs::read(&battery).unwrap().len(), 8192);
        assert_eq!(fs::read(&kept).unwrap(), b"not a battery save");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_autosave_that_cannot_be_read_is_reported_and_pauses() {
        let dir = temp_dir("session-autoerr");
        let (library, game) = library_with_game(&dir);
        // A directory where the autosave belongs: it exists, and it will never read.
        fs::create_dir_all(library.state_path(&game.id, Slot::Auto)).unwrap();
        let (session, warnings) = Session::open(&library, game.clone(), None).unwrap();
        assert_eq!(warnings, vec![START_EARLIER.to_string()]);
        assert!(session.paused);
        assert!(library.problems()[0].contains("auto.state"));
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
        assert!(session
            .flush_battery()
            .unwrap_err()
            .contains(&blocked.display().to_string()));
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
        fs::write(
            dir.join(format!("{}.sav", engine.identity())),
            vec![0x5a; 8192],
        )
        .unwrap();
        let state = engine.save_state();
        fs::write(dir.join(format!("{}.state", engine.identity())), &state).unwrap();
        let (session, warnings) = Session::open(&library, game.clone(), None).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(session.engine.battery_ram().unwrap()[0], 0x5a);
        assert_eq!(
            fs::read(library.state_path(&game.id, Slot::Number(0))).unwrap(),
            state
        );
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
        assert!(path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("Test "));
        let (w, h, _) = crate::files::read_png(&path).unwrap();
        assert_eq!((w, h), (256, 240));
        fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn regression_unreadable_valid_auto_survives_close_without_playing() {
        use nes_core::cpu::Bus;
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("review-unreadable-auto");
        let (library, game) = library_with_game(&dir);
        let (mut s, _) = Session::open(&library, game.clone(), None).unwrap();
        s.engine.nes.bus.write(0x6000, 0x5a);
        let original = s.engine.save_state();
        let auto = library.state_path(&game.id, Slot::Auto);
        fs::write(&auto, &original).unwrap();
        fs::set_permissions(&auto, fs::Permissions::from_mode(0o000)).unwrap();
        assert!(fs::read(&auto).is_err());
        drop(s);
        let (mut s, warnings) = Session::open(&library, game.clone(), None).unwrap();
        assert_eq!(warnings, vec![START_EARLIER.to_string()]);
        assert!(s.paused);
        s.close().unwrap();
        let kept = PathBuf::from(format!("{}.unreadable", auto.display()));
        fs::set_permissions(&kept, fs::Permissions::from_mode(0o600)).unwrap();
        let saved = fs::read(&kept).unwrap();
        let mut probe = Engine::new(&test_rom()).unwrap();
        probe.load_state(&saved).unwrap();
        eprintln!(
            "Unreadable valid auto, close without playing: battery {} -> {}",
            0x5a,
            probe.battery_ram().unwrap()[0]
        );
        assert!(
            saved == original,
            "closing without playing destroyed the unreadable valid autosave"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn regression_newer_flushed_battery_survives_unclean_restart() {
        use nes_core::cpu::Bus;
        let dir = temp_dir("review-newer-battery");
        let (library, game) = library_with_game(&dir);
        let (mut s, _) = Session::open(&library, game.clone(), None).unwrap();
        s.engine.nes.bus.write(0x6000, 0x11);
        s.save(Slot::Auto).unwrap();
        s.engine.nes.bus.write(0x6000, 0x77);
        s.flush_battery().unwrap();
        drop(s); // An exit without Session::close(), after a successful battery flush.
        assert_eq!(fs::read(library.battery_path(&game.id)).unwrap()[0], 0x77);
        let (mut s, warnings) = Session::open(&library, game.clone(), None).unwrap();
        assert_eq!(warnings, vec![START_EARLIER.to_string()]);
        assert!(s.paused);
        let restored = s.engine.battery_ram().unwrap()[0];
        s.close().unwrap();
        let on_disk = fs::read(library.battery_path(&game.id)).unwrap()[0];
        eprintln!("Newer battery: disk before restart=119, restored={restored}, disk after close={on_disk}");
        assert_eq!(
            on_disk, 0x77,
            "older autosave overwrote the successfully flushed battery"
        );
        assert_eq!(restored, 0x77);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn regression_rewind_to_migrated_start_keeps_loaded_battery() {
        let dir = temp_dir("review-migrated-rewind");
        let (library, game) = library_with_game(&dir);
        let identity = Engine::new(&test_rom()).unwrap().identity();
        fs::write(dir.join(format!("{identity}.sav")), vec![0x5a; 8192]).unwrap();
        let (mut s, warnings) = Session::open(&library, game.clone(), None).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(s.engine.battery_ram().unwrap()[0], 0x5a);
        s.engine.step(Buttons(0), Buttons(0));
        assert!(s.engine.rewind_step());
        s.close().unwrap();
        let on_disk = fs::read(library.battery_path(&game.id)).unwrap()[0];
        eprintln!("Migrated battery after one step and one rewind: 90 -> {on_disk}");
        assert_eq!(
            on_disk, 0x5a,
            "rewind returned to the pre-battery-load anchor"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn regression_close_attempts_battery_when_auto_write_fails() {
        use nes_core::cpu::Bus;
        let dir = temp_dir("review-close-auto-failure");
        let (library, game) = library_with_game(&dir);
        let (mut s, _) = Session::open(&library, game.clone(), None).unwrap();
        s.engine.nes.bus.write(0x6000, 0x11);
        s.flush_battery().unwrap();
        s.engine.nes.bus.write(0x6000, 0x77);
        let auto = library.state_path(&game.id, Slot::Auto);
        fs::create_dir_all(&auto).unwrap(); // Only auto.state is blocked; battery.sav is writable.
        assert!(s.close().is_err());
        let on_disk = fs::read(library.battery_path(&game.id)).unwrap()[0];
        eprintln!("Failed autosave on close: RAM=119, persisted battery={on_disk}");
        assert_eq!(
            on_disk, 0x77,
            "auto error short-circuited the final battery flush"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn recovery_copies_are_not_replaced_by_another_failed_open() {
        let dir = temp_dir("recovery-copies");
        let (library, game) = library_with_game(&dir);
        let auto = library.state_path(&game.id, Slot::Auto);
        for bytes in [b"first failed state", b"other failed state"] {
            fs::write(&auto, bytes).unwrap();
            let (mut session, warnings) = Session::open(&library, game.clone(), None).unwrap();
            assert_eq!(warnings, vec![START_EARLIER.to_string()]);
            session.close().unwrap();
        }
        assert_eq!(
            fs::read(auto.with_file_name("auto.state.unreadable")).unwrap(),
            b"first failed state"
        );
        assert_eq!(
            fs::read(auto.with_file_name("auto.state.unreadable-1")).unwrap(),
            b"other failed state"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn protected_autosave_is_kept_before_retrying_after_directory_recovers() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("protected-auto");
        let (library, game) = library_with_game(&dir);
        let auto = library.state_path(&game.id, Slot::Auto);
        fs::write(&auto, b"recoverable state").unwrap();
        let folder = library.directory(&game.id);
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();
        let opened = Session::open(&library, game.clone(), None);
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
        let (mut session, _) = opened.unwrap();
        assert!(session.auto_unreadable);
        assert_eq!(fs::read(&auto).unwrap(), b"recoverable state");
        session.close().unwrap();
        assert_eq!(
            fs::read(auto.with_file_name("auto.state.unreadable")).unwrap(),
            b"recoverable state"
        );
        assert!(!session.auto_unreadable);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_battery_write_does_not_commit_a_newer_autosave() {
        use nes_core::cpu::Bus;
        let dir = temp_dir("battery-before-state");
        let (library, game) = library_with_game(&dir);
        let (mut session, _) = Session::open(&library, game.clone(), None).unwrap();
        session.save(Slot::Auto).unwrap();
        let auto = library.state_path(&game.id, Slot::Auto);
        let original = fs::read(&auto).unwrap();
        session.engine.nes.bus.write(0x6000, 0x5a);
        let blocked = library
            .battery_path(&game.id)
            .with_extension(format!("tmp-{}", std::process::id()));
        fs::write(&blocked, b"conflict").unwrap();
        assert!(session.close().is_err());
        assert_eq!(fs::read(&auto).unwrap(), original);
        fs::remove_file(blocked).unwrap();
        session.close().unwrap();
        let (session, warnings) = Session::open(&library, game.clone(), None).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(session.engine.battery_ram().unwrap()[0], 0x5a);
        fs::remove_dir_all(dir).unwrap();
    }
}
