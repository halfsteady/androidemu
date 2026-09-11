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
    /// The saved settings, and the reason when the file could not be read.
    ///
    /// A file that will not parse is not a file to throw away. The shell runs
    /// on the defaults and says so, and nothing writes over the original until
    /// somebody changes a setting on purpose — which is the only way anybody
    /// gets the chance to fix a stray comma by hand.
    pub fn load(dir: &Path) -> (Settings, Option<String>) {
        let path = dir.join(SETTINGS);
        let bytes = match read_optional(&path) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return (Settings::default(), None),
            Err(e) => return (Settings::default(), Some(e)),
        };
        match serde_json::from_slice::<OnDisk>(&bytes) {
            Ok(disk) => (
                Settings {
                    aspect: Aspect::from_index(disk.aspect),
                    look: Look::from_id(disk.look),
                    palette: PaletteChoice::from_id(disk.palette),
                    trim_edges: disk.trim_edges,
                    shelf_list: disk.shelf_list,
                    fullscreen: disk.fullscreen,
                },
                None,
            ),
            Err(e) => (
                Settings::default(),
                Some(format!("{}: {e}", path.display())),
            ),
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

    /// The colours the picture is painted with, and the reason when the chosen
    /// palette could not be honoured.
    ///
    /// Standard is the core's own table and the built-in palettes are built
    /// rather than read, so From a file is the only choice that can go
    /// missing: it is a file on somebody's disk that they can move, replace or
    /// truncate. Falling back to Standard without a word would be a picture
    /// that quietly changed colour, so the reason comes back with the table.
    pub fn colours(&self, dir: &Path) -> ([u32; 64], Option<String>) {
        match self.palette {
            PaletteChoice::Standard => (crate::palette::PALETTE, None),
            PaletteChoice::File => match read_palette(dir) {
                Ok(table) => (table, None),
                Err(why) => (crate::palette::PALETTE, Some(why)),
            },
            other => (other.colours().unwrap_or(crate::palette::PALETTE), None),
        }
    }
}

/// The imported palette, or why it cannot be used. The two ways it goes wrong
/// read differently to somebody looking at the problem log: a file that is not
/// there any more is not the same as one that is there and is not a palette.
fn read_palette(dir: &Path) -> Result<[u32; 64], String> {
    let path = dir.join(PALETTE_FILE);
    match read_optional(&path) {
        Ok(Some(bytes)) => {
            model::parse(&bytes).ok_or_else(|| format!("{}: not a palette", path.display()))
        }
        Ok(None) => Err(format!("{}: not there any more", path.display())),
        Err(e) => Err(e),
    }
}

pub fn imported_palette(dir: &Path) -> Option<[u32; 64]> {
    read_palette(dir).ok()
}

pub fn store_palette(dir: &Path, bytes: &[u8]) -> Result<[u32; 64], String> {
    let table = model::parse(bytes).ok_or("That palette file didn't work.")?;
    write_atomic(&dir.join(PALETTE_FILE), bytes)?;
    Ok(table)
}

/// Every field takes a default, so one profile written by an older shell — or
/// edited by hand and missing a line — loses that one button rather than
/// taking the whole file's worth of controllers down with it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Profile {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub a: String,
    #[serde(default)]
    pub b: String,
    #[serde(default)]
    pub select: String,
    #[serde(default)]
    pub start: String,
}

#[derive(Default)]
pub struct Profiles(pub BTreeMap<String, Profile>);

impl Profiles {
    pub const KEYBOARD: &'static str = "keyboard";

    /// The saved button profiles, and the reason when the file could not be
    /// read. Left intact and run without, the same way the settings are: these
    /// are four presses each and somebody's to keep.
    pub fn load(dir: &Path) -> (Profiles, Option<String>) {
        let path = dir.join(CONTROLLERS);
        let bytes = match read_optional(&path) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return (Profiles::default(), None),
            Err(e) => return (Profiles::default(), Some(e)),
        };
        match serde_json::from_slice(&bytes) {
            Ok(map) => (Profiles(map), None),
            Err(e) => (
                Profiles::default(),
                Some(format!("{}: {e}", path.display())),
            ),
        }
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
        let (s, problem) = Settings::load(&dir);
        assert_eq!(problem, None);
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
        let s = Settings {
            aspect: Aspect::Pixels,
            look: Look::Composite,
            palette: PaletteChoice::Vivid,
            trim_edges: true,
            shelf_list: true,
            fullscreen: true,
        };
        s.save(&dir).unwrap();
        let text = fs::read_to_string(dir.join("settings.json")).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["look"], 10);
        assert_eq!(json["palette"], 3);
        assert_eq!(json["aspect"], 2);
        assert_eq!(Settings::load(&dir).0, s);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn retired_ids_and_unknown_keys_are_tolerated() {
        let dir = temp_dir("settings-retired");
        fs::write(
            dir.join("settings.json"),
            r#"{"look": 5, "palette": 4, "aspect": 7, "future": true}"#,
        )
        .unwrap();
        let (s, problem) = Settings::load(&dir);
        assert_eq!(problem, None);
        assert_eq!(s.look, Look::OldPhoto);
        assert_eq!(s.palette, PaletteChoice::Standard);
        assert_eq!(s.aspect, Aspect::Television);
        fs::remove_dir_all(dir).unwrap();
    }

    /// A settings file nobody can parse is somebody's file all the same. The
    /// shell runs on the defaults, says which file it was, and leaves what is
    /// on disk exactly where it is — a save would otherwise write over the
    /// stray comma before anybody had the chance to find it.
    #[test]
    fn a_malformed_settings_file_is_reported_and_left_alone() {
        let dir = temp_dir("settings-malformed");
        let path = dir.join("settings.json");
        fs::write(&path, "not json").unwrap();
        let (s, problem) = Settings::load(&dir);
        assert_eq!(s, Settings::default());
        let problem = problem.expect("a file that will not parse has a reason");
        assert!(problem.contains("settings.json"), "{problem}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "not json");
        // And the same for the controllers, which are four presses each.
        let path = dir.join("controllers.json");
        fs::write(&path, "{\"keyboard\": ").unwrap();
        let (p, problem) = Profiles::load(&dir);
        assert!(p.get(Profiles::KEYBOARD).is_none());
        let problem = problem.expect("a file that will not parse has a reason");
        assert!(problem.contains("controllers.json"), "{problem}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"keyboard\": ");
        fs::remove_dir_all(dir).unwrap();
    }

    /// One profile short of a button is one button short, not a file's worth
    /// of controllers thrown away.
    #[test]
    fn a_profile_missing_a_button_keeps_the_rest_of_the_file() {
        let dir = temp_dir("settings-partial");
        fs::write(
            dir.join("controllers.json"),
            r#"{"keyboard": {"name": "Keyboard", "a": "X", "b": "Z", "start": "Return"},
                "030000": {"name": "Pad", "a": "b", "b": "a", "select": "back", "start": "start"}}"#,
        )
        .unwrap();
        let (p, problem) = Profiles::load(&dir);
        assert_eq!(problem, None);
        let keyboard = p.get(Profiles::KEYBOARD).unwrap();
        assert_eq!(keyboard.a, "X");
        assert_eq!(keyboard.select, "");
        assert_eq!(p.get("030000").unwrap().select, "back");
        fs::remove_dir_all(dir).unwrap();
    }

    /// A palette file that has gone falls back to Standard and comes back
    /// with the reason, which is what the shell says out loud: the picture
    /// changing colour on its own is the thing to avoid.
    #[test]
    fn a_missing_imported_palette_falls_back_to_standard_and_says_why() {
        let dir = temp_dir("settings-palette");
        let s = Settings {
            palette: PaletteChoice::File,
            ..Settings::default()
        };
        let (table, why) = s.colours(&dir);
        assert_eq!(table, crate::palette::PALETTE);
        assert!(why.unwrap().contains("palette.pal"));
        assert!(imported_palette(&dir).is_none());
        assert!(store_palette(&dir, &[0; 100]).is_err());
        // There but not a palette is a different sentence from not there.
        fs::write(dir.join("palette.pal"), [0u8; 100]).unwrap();
        let (table, why) = s.colours(&dir);
        assert_eq!(table, crate::palette::PALETTE);
        assert!(why.unwrap().contains("not a palette"));
        let table = model::build(1.1, 0.0, 1.0, 0.0, 1.0);
        assert_eq!(store_palette(&dir, &model::bytes(&table)).unwrap(), table);
        assert_eq!(imported_palette(&dir), Some(table));
        assert_eq!(s.colours(&dir), (table, None));
        // Nothing else can go missing: Standard is the core's own table and
        // the rest are built rather than read.
        assert_eq!(
            Settings::default().colours(&dir),
            (crate::palette::PALETTE, None)
        );
        assert_eq!(
            Settings {
                palette: PaletteChoice::Soft,
                ..Settings::default()
            }
            .colours(&dir),
            (PaletteChoice::Soft.colours().unwrap(), None)
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn controller_profiles_round_trip() {
        let dir = temp_dir("settings-profiles");
        let (mut p, problem) = Profiles::load(&dir);
        assert_eq!(problem, None);
        assert!(p.get(Profiles::KEYBOARD).is_none());
        p.set(
            "030000".into(),
            Profile {
                name: "Pad".into(),
                a: "b".into(),
                b: "a".into(),
                select: "back".into(),
                start: "start".into(),
            },
        );
        p.save(&dir).unwrap();
        let (back, _) = Profiles::load(&dir);
        assert_eq!(back.get("030000").unwrap().name, "Pad");
        assert_eq!(back.get("030000").unwrap().a, "b");
        fs::remove_dir_all(dir).unwrap();
    }
}
