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
    read_optional(&dir.join(PALETTE_FILE))
        .ok()
        .flatten()
        .and_then(|b| model::parse(&b))
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
        assert_eq!(Settings::load(&dir), s);
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
        let s = Settings {
            palette: PaletteChoice::File,
            ..Settings::default()
        };
        assert_eq!(s.colours(&dir), crate::palette::PALETTE);
        assert!(imported_palette(&dir).is_none());
        assert!(store_palette(&dir, &[0; 100]).is_err());
        let table = model::build(1.1, 0.0, 1.0, 0.0, 1.0);
        assert_eq!(store_palette(&dir, &model::bytes(&table)).unwrap(), table);
        assert_eq!(imported_palette(&dir), Some(table));
        assert_eq!(s.colours(&dir), table);
        assert_eq!(Settings::default().colours(&dir), crate::palette::PALETTE);
        assert_eq!(Settings::default().preview_colours(&dir), model::standard());
        assert_eq!(
            Settings {
                palette: PaletteChoice::Soft,
                ..Settings::default()
            }
            .colours(&dir),
            PaletteChoice::Soft.colours().unwrap()
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn controller_profiles_round_trip() {
        let dir = temp_dir("settings-profiles");
        let mut p = Profiles::load(&dir);
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
        let back = Profiles::load(&dir);
        assert_eq!(back.get("030000").unwrap().name, "Pad");
        assert_eq!(back.get("030000").unwrap().a, "b");
        fs::remove_dir_all(dir).unwrap();
    }
}
