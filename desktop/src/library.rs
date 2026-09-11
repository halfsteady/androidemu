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

    /// The same shelf, adopted from a folder someone else already made.
    pub fn from_root(root: PathBuf) -> Library {
        Library { root }
    }

    /// The `library` folder itself: every game's folder lives inside it.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The data folder the shelf sits in, where the shell keeps everything else.
    pub fn data_dir(&self) -> PathBuf {
        self.root.parent().unwrap_or(&self.root).to_path_buf()
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

    /// Put away rather than deleted, newest first.
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
        let game = Game {
            id: id.into(),
            title: title.into(),
            added: now_millis(),
            played: 0,
            seconds: 0,
            archived: false,
        };
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

    /// Takes a game off the shelf, or puts it back. Nothing on disk is touched,
    /// so bringing a game back returns it mid-adventure.
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

    /// Where a game keeps its ROM, battery save, states and art. Only a path:
    /// asking for it must not bring the folder back after `forget`.
    pub fn directory(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    /// The same folder, made on the way, for the paths about to be written to.
    fn folder(&self, id: &str) -> PathBuf {
        let dir = self.directory(id);
        let _ = fs::create_dir_all(&dir);
        dir
    }

    pub fn rom_path(&self, id: &str) -> PathBuf {
        self.folder(id).join("game.nes")
    }
    pub fn battery_path(&self, id: &str) -> PathBuf {
        self.folder(id).join("battery.sav")
    }
    pub fn state_path(&self, id: &str, slot: Slot) -> PathBuf {
        self.folder(id).join(match slot {
            Slot::Auto => "auto.state".to_string(),
            Slot::Number(n) => format!("slot-{n}.state"),
        })
    }
    pub fn thumbnail_path(&self, id: &str, slot: Slot) -> PathBuf {
        self.folder(id).join(match slot {
            Slot::Auto => "auto.png".to_string(),
            Slot::Number(n) => format!("slot-{n}.png"),
        })
    }
    /// Box art chosen by hand. It outranks the saved screenshot on the shelf.
    pub fn art_path(&self, id: &str) -> PathBuf {
        self.folder(id).join("art.png")
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
        [self.art_path(id), self.thumbnail_path(id, Slot::Auto)]
            .into_iter()
            .find(|p| p.exists())
    }

    /// Box art: decoded, shrunk so the longest edge is at most 1024, stored as PNG.
    pub fn set_art(&self, id: &str, image_path: &Path) -> Result<(), String> {
        let image =
            image::open(image_path).map_err(|e| format!("{}: {e}", image_path.display()))?;
        let image = if image.width().max(image.height()) > ART_MAX_EDGE {
            image.resize(
                ART_MAX_EDGE,
                ART_MAX_EDGE,
                image::imageops::FilterType::Triangle,
            )
        } else {
            image
        };
        let rgba = image.to_rgba8();
        write_png(
            &self.art_path(id),
            rgba.width(),
            rgba.height(),
            rgba.as_raw(),
        )
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

    /// Newest first, because the problem someone is asking about just happened.
    pub fn problems(&self) -> Vec<String> {
        let text = fs::read_to_string(self.problems_path()).unwrap_or_default();
        text.lines().rev().map(String::from).collect()
    }
}

/// A file the person picked: the title is its name without the extension,
/// and the bytes are bounded because nothing 16 MB long is a game.
pub fn read_import(path: &Path) -> Result<(String, Vec<u8>), String> {
    let title = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "My game".into());
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(MAX_ROM as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_ROM {
        return Err("This file is too large to be a game".into());
    }
    Ok((title, bytes))
}

/// How long this game has been played, in words, or nothing under a minute.
pub fn playtime(seconds: i64) -> Option<String> {
    match seconds {
        s if s < 60 => None,
        s if s < 3600 => Some(format!("{} min played", s / 60)),
        s => Some(format!("{} h {} min played", s / 3600, (s % 3600) / 60)),
    }
}

/// A cover or thumbnail as RGBA8, or nothing if it cannot be read.
pub fn thumbnail_pixels(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    read_png(path).ok()
}

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
        fs::write(
            dir.join("library/index.json"),
            r#"[{"id":"abcd","title":"Old","added":5}]"#,
        )
        .unwrap();
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
        assert_eq!(
            lib.state_path("aaaa", Slot::Auto),
            lib.directory("aaaa").join("auto.state")
        );
        assert_eq!(
            lib.state_path("aaaa", Slot::Number(3)),
            lib.directory("aaaa").join("slot-3.state")
        );
        assert_eq!(
            lib.thumbnail_path("aaaa", Slot::Number(0)),
            lib.directory("aaaa").join("slot-0.png")
        );
        assert_eq!(
            lib.battery_path("aaaa"),
            lib.directory("aaaa").join("battery.sav")
        );
        let slots = lib.slots("aaaa");
        assert_eq!(slots.len(), 10);
        assert!(slots.iter().all(|s| s.time.is_none()));
        assert!(lib.cover("aaaa").is_none());
        assert!(!lib.has_autosave("aaaa"));
        fs::write(lib.state_path("aaaa", Slot::Number(2)), b"state").unwrap();
        assert!(lib.slots("aaaa")[2].time.is_some());
        crate::files::write_png(
            &lib.thumbnail_path("aaaa", Slot::Auto),
            1,
            1,
            &[1, 2, 3, 255],
        )
        .unwrap();
        fs::write(lib.state_path("aaaa", Slot::Auto), b"state").unwrap();
        assert!(lib.has_autosave("aaaa"));
        assert_eq!(
            lib.cover("aaaa"),
            Some(lib.thumbnail_path("aaaa", Slot::Auto))
        );
        let big: Vec<u8> = vec![200; 2048 * 512 * 4];
        let source = dir.join("art.png");
        crate::files::write_png(&source, 2048, 512, &big).unwrap();
        lib.set_art("aaaa", &source).unwrap();
        assert_eq!(lib.cover("aaaa"), Some(lib.art_path("aaaa")));
        let (w, h, _) = crate::files::read_png(&lib.art_path("aaaa")).unwrap();
        assert_eq!((w, h), (1024, 256));
        lib.clear_art("aaaa").unwrap();
        assert_eq!(
            lib.cover("aaaa"),
            Some(lib.thumbnail_path("aaaa", Slot::Auto))
        );
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
        fs::File::create(&huge)
            .unwrap()
            .set_len(MAX_ROM as u64 + 1)
            .unwrap();
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
