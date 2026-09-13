//! Player-one mappings for the opt-in Dolphin host. Physical input is kept out
//! of Dolphin's configuration files; the bridge supplies a virtual device.
use sdl2::{
    controller::{Axis, Button, GameController},
    event::Event,
    keyboard::Scancode,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
};

pub const STYLES: [&str; 4] = [
    "GameCube controller",
    "Sideways Wii Remote",
    "Wii Remote + Nunchuk",
    "Classic controller",
];
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Physical {
    Key(String),
    Button(String),
    Axis { name: String, negative: bool },
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Binding {
    pub keyboard: Option<Physical>,
    pub gamepad: Option<Physical>,
}
#[derive(Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub style: usize,
    pub maps: BTreeMap<String, Binding>,
    #[serde(default = "mouse_default")]
    pub mouse: bool,
}
fn mouse_default() -> bool {
    true
}
impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            style: 0,
            maps: BTreeMap::new(),
            mouse: true,
        }
    }
}
pub struct Controls {
    pub config: Config,
    path: PathBuf,
    pub schema: Vec<String>,
    pub capture: Option<(String, bool)>,
    pads: BTreeMap<u32, GameController>,
    keys: HashSet<String>,
    buttons: HashSet<String>,
    axes: HashMap<String, i16>,
    pub dirty: bool,
    mouse_position: Option<[f64; 2]>,
    mouse_aim: Option<[f64; 2]>,
    mouse_inside: bool,
    mouse_buttons: u32,
    mouse_armed: bool,
}
impl Controls {
    pub fn open(path: &Path) -> Result<Self, String> {
        let config = match crate::files::read_optional(path)? {
            Some(bytes) => serde_json::from_slice::<Config>(&bytes).map_err(|e| e.to_string())?,
            None => Config::default(),
        };
        if config.version != 1 || config.style >= STYLES.len() {
            return Err("Unsupported Dolphin controls file".into());
        }
        Ok(Self {
            config,
            path: path.into(),
            schema: Vec::new(),
            capture: None,
            pads: BTreeMap::new(),
            keys: HashSet::new(),
            buttons: HashSet::new(),
            axes: HashMap::new(),
            dirty: false,
            mouse_position: None,
            mouse_aim: None,
            mouse_inside: false,
            mouse_buttons: 0,
            mouse_armed: false,
        })
    }
    pub fn connect(&mut self, subsystem: &sdl2::GameControllerSubsystem) {
        for i in 0..subsystem.num_joysticks().unwrap_or(0) {
            self.add(subsystem, i);
        }
    }
    fn add(&mut self, subsystem: &sdl2::GameControllerSubsystem, index: u32) {
        if let Ok(pad) = subsystem.open(index) {
            self.pads.insert(pad.instance_id(), pad);
        }
    }
    pub fn save(&mut self) -> Result<(), String> {
        crate::files::write_atomic(
            &self.path,
            &serde_json::to_vec_pretty(&self.config).map_err(|e| e.to_string())?,
        )?;
        self.dirty = false;
        Ok(())
    }
    #[cfg(any(debug_assertions, test))]
    pub fn qualify_input(&mut self, kind: &str, name: &str, value: i16) {
        match kind {
            "key" => {
                if value != 0 {
                    self.keys.insert(name.into());
                } else {
                    self.keys.remove(name);
                }
            }
            "axis" => {
                self.axes.insert(name.into(), value);
                if i32::from(value).abs() > 6000 {
                    self.mouse_aim = None;
                }
            }
            _ => {}
        }
    }
    pub fn pointer(&mut self, position: [f64; 2], buttons: u32, inside: bool) {
        if buttons == 0 {
            self.mouse_armed = true;
        }
        self.mouse_inside = inside;
        let buttons = if inside && self.mouse_armed {
            buttons
        } else {
            0
        };
        if inside && (self.mouse_position != Some(position) || buttons != self.mouse_buttons) {
            self.mouse_aim = Some(position);
            self.mouse_position = Some(position);
        }
        self.mouse_buttons = buttons;
    }
    pub fn clear(&mut self) {
        self.mouse_position = None;
        self.mouse_aim = None;
        self.mouse_buttons = 0;
        self.mouse_inside = false;
        self.mouse_armed = false;
        self.keys.clear();
        self.buttons.clear();
        self.axes.clear();
    }
    pub fn event(
        &mut self,
        event: &Event,
        subsystem: &sdl2::GameControllerSubsystem,
        playing: bool,
    ) {
        let first = self.pads.keys().next().copied();
        let mut candidate = None;
        match event {
            Event::ControllerDeviceAdded { which, .. } => self.add(subsystem, *which),
            Event::ControllerDeviceRemoved { which, .. } => {
                self.pads.remove(which);
                self.clear();
            }
            Event::KeyDown {
                scancode: Some(key),
                repeat: false,
                ..
            } if *key != Scancode::Escape => {
                let name = key.name().to_owned();
                if ["Wii/IR/Up", "Wii/IR/Down", "Wii/IR/Left", "Wii/IR/Right"]
                    .iter()
                    .any(|control| {
                        self.config
                            .maps
                            .get(*control)
                            .cloned()
                            .unwrap_or_else(|| default_binding(control))
                            .keyboard
                            == Some(Physical::Key(name.clone()))
                    })
                {
                    self.mouse_aim = None;
                }
                self.keys.insert(name.clone());
                candidate = Some(Physical::Key(name));
            }
            Event::KeyUp {
                scancode: Some(key),
                ..
            } => {
                self.keys.remove(key.name());
            }
            Event::ControllerButtonDown { which, button, .. } if Some(*which) == first => {
                let name = button.string();
                self.buttons.insert(name.clone());
                candidate = Some(Physical::Button(name));
            }
            Event::ControllerButtonUp { which, button, .. } if Some(*which) == first => {
                self.buttons.remove(&button.string());
            }
            Event::ControllerAxisMotion {
                which, axis, value, ..
            } if Some(*which) == first => {
                self.axes.insert(axis.string(), *value);
                if i32::from(*value).abs() > 6000 {
                    self.mouse_aim = None;
                }
                if i32::from(*value).abs() > 24000 {
                    candidate = Some(Physical::Axis {
                        name: axis.string(),
                        negative: *value < 0,
                    });
                }
            }
            _ => {}
        }
        if let (Some((name, keyboard)), Some(physical)) = (&self.capture, candidate) {
            if *keyboard == matches!(physical, Physical::Key(_)) {
                let default = self.binding(name);
                let binding = self.config.maps.entry(name.clone()).or_insert(default);
                if *keyboard {
                    binding.keyboard = Some(physical);
                } else {
                    binding.gamepad = Some(physical);
                }
                self.capture = None;
                self.dirty = true;
                self.clear();
            }
        }
        if !playing {
            self.clear();
        }
    }
    fn binding(&self, name: &str) -> Binding {
        self.config.maps.get(name).cloned().unwrap_or_else(|| {
            if (name.starts_with("Wii/Tilt/") && self.config.style != 1)
                || name.starts_with("Nunchuk/Tilt/")
            {
                Binding::default()
            } else {
                default_binding(name)
            }
        })
    }
    pub fn value(&self, name: &str) -> f64 {
        let binding = self.binding(name);
        let physical = [binding.keyboard, binding.gamepad]
            .iter()
            .filter_map(Option::as_ref)
            .map(|p| match p {
                Physical::Key(key) => f64::from(self.keys.contains(key)),
                Physical::Button(button) => f64::from(self.buttons.contains(button)),
                Physical::Axis { name, negative } => {
                    axis_value(*self.axes.get(name).unwrap_or(&0), *negative)
                }
            })
            .fold(0.0, f64::max);
        if !self.config.mouse {
            return physical;
        }
        if matches!(name, "Wii/Buttons/A" | "Classic/Buttons/A" | "GC/Buttons/A") {
            return physical.max(f64::from(self.mouse_buttons & 1 != 0));
        }
        if matches!(name, "Wii/Buttons/B" | "Classic/Buttons/B" | "GC/Buttons/B") {
            return physical.max(f64::from(self.mouse_buttons & 2 != 0));
        }
        if let Some([x, y]) = self.mouse_aim {
            match name {
                "Wii/IR/Right" => return x.clamp(0.0, 1.0),
                "Wii/IR/Left" => return (-x).clamp(0.0, 1.0),
                "Wii/IR/Up" => return y.clamp(0.0, 1.0),
                "Wii/IR/Down" => return (-y).clamp(0.0, 1.0),
                "Wii/IR/Hide" => return f64::from(!self.mouse_inside),
                _ => {}
            }
        }
        physical
    }
    pub fn show(&mut self, ctx: &egui::Context, running_style: usize, wii: bool) -> bool {
        crate::ui::widgets::panel(
            ctx,
            "dolphin-controls",
            "Dolphin controls",
            Some("Player one · saved for this game"),
            true,
            |ui| {
                if wii {
                    self.dirty |= ui
                        .checkbox(
                            &mut self.config.mouse,
                            "Mouse aims Wii Remote · left click A · right click B",
                        )
                        .changed();
                }
                let old = self.config.style;
                egui::ComboBox::from_id_salt("dolphin-style")
                    .selected_text(STYLES[self.config.style])
                    .show_ui(ui, |ui| {
                        for (i, label) in STYLES.iter().enumerate().take(if wii { 4 } else { 1 }) {
                            ui.selectable_value(&mut self.config.style, i, *label);
                        }
                    });
                self.dirty |= old != self.config.style;
                if running_style != self.config.style {
                    ui.label("Restart the game to use this controller style.");
                }
                ui.label(
                    self.pads
                        .values()
                        .next()
                        .map(|p| format!("Gamepad: {}", p.name()))
                        .unwrap_or_else(|| "Connect a gamepad, or use the keyboard.".into()),
                );
                if let Some((name, keyboard)) = &self.capture {
                    ui.label(format!(
                        "{name}: press {} · Escape cancels",
                        if *keyboard {
                            "a key"
                        } else {
                            "a button or move an axis"
                        }
                    ));
                }
                let prefixes: &[&str] = match self.config.style {
                    0 => &["GC/"],
                    1 => &["Wii/"],
                    2 => &["Wii/", "Nunchuk/"],
                    _ => &["Wii/", "Classic/"],
                };
                for name in &self.schema {
                    if !prefixes.iter().any(|p| name.starts_with(p))
                        || name.contains("IMU")
                        || name.contains("Hotkeys")
                        || name.contains("IRPassthrough")
                        || name.contains("Triforce")
                        || name.contains("Microphone")
                    {
                        continue;
                    }
                    let binding = self.binding(name);
                    ui.push_id(name, |ui| {
                        ui.label(
                            name.trim_start_matches("GC/")
                                .trim_start_matches("Wii/")
                                .replace('/', " · "),
                        );
                        ui.horizontal(|ui| {
                            if ui.button(label(&binding.keyboard, "Key")).clicked() {
                                self.capture = Some((name.clone(), true));
                            }
                            if ui.button(label(&binding.gamepad, "Gamepad")).clicked() {
                                self.capture = Some((name.clone(), false));
                            }
                            if ui.small_button("Clear").clicked() {
                                self.config.maps.insert(name.clone(), Binding::default());
                                self.dirty = true;
                            }
                        });
                    });
                }
                if ui.button("Restore this game's defaults").clicked() {
                    self.config.maps.clear();
                    self.dirty = true;
                }
            },
        )
    }
}
fn label(binding: &Option<Physical>, empty: &str) -> String {
    match binding {
        None => format!("{empty}: unset"),
        Some(Physical::Key(name) | Physical::Button(name)) => name.clone(),
        Some(Physical::Axis { name, negative }) => {
            format!("{name} {}", if *negative { "−" } else { "+" })
        }
    }
}
fn axis_value(value: i16, negative: bool) -> f64 {
    let signed = f64::from(value) * if negative { -1.0 } else { 1.0 } / 32767.0;
    ((signed - 0.15) / 0.85).clamp(0.0, 1.0)
}
fn default_binding(name: &str) -> Binding {
    let mut parts = name.split('/');
    let system = parts.next().unwrap_or("");
    let group = parts.next().unwrap_or("");
    let control = parts.next().unwrap_or("");
    let pair = match (group, control) {
        ("Buttons", "A") => (Some(Scancode::X), Some(Button::A)),
        ("Buttons", "B") => (Some(Scancode::Z), Some(Button::B)),
        ("Buttons", "X" | "1") => (Some(Scancode::C), Some(Button::X)),
        ("Buttons", "Y" | "2") => (Some(Scancode::V), Some(Button::Y)),
        ("Buttons", "Start" | "+") => (Some(Scancode::Return), Some(Button::Start)),
        ("Buttons", "-" | "Home") => (Some(Scancode::Backspace), Some(Button::Back)),
        ("Buttons", "Z" | "ZR") => (Some(Scancode::F), Some(Button::RightShoulder)),
        ("Buttons", "C" | "ZL") => (Some(Scancode::LShift), Some(Button::LeftShoulder)),
        ("D-Pad", "Up") => (Some(Scancode::Up), Some(Button::DPadUp)),
        ("D-Pad", "Down") => (Some(Scancode::Down), Some(Button::DPadDown)),
        ("D-Pad", "Left") => (Some(Scancode::Left), Some(Button::DPadLeft)),
        ("D-Pad", "Right") => (Some(Scancode::Right), Some(Button::DPadRight)),
        _ => (None, None),
    };
    let mut result = Binding {
        keyboard: pair.0.map(|k| Physical::Key(k.name().into())),
        gamepad: pair.1.map(|b| Physical::Button(b.string())),
    };
    let axis = if [
        "Main Stick",
        "C-Stick",
        "Stick",
        "Left Stick",
        "Right Stick",
        "IR",
        "Tilt",
    ]
    .contains(&group)
    {
        let right = matches!(group, "C-Stick" | "Right Stick" | "IR");
        match control {
            "Up" | "Forward" => Some((
                if right { Axis::RightY } else { Axis::LeftY },
                true,
                Scancode::I,
            )),
            "Down" | "Backward" => Some((
                if right { Axis::RightY } else { Axis::LeftY },
                false,
                Scancode::K,
            )),
            "Left" => Some((
                if right { Axis::RightX } else { Axis::LeftX },
                true,
                Scancode::J,
            )),
            "Right" => Some((
                if right { Axis::RightX } else { Axis::LeftX },
                false,
                Scancode::L,
            )),
            _ => None,
        }
    } else if group == "Triggers" {
        match control {
            "L" | "L-Analog" => Some((Axis::TriggerLeft, false, Scancode::Q)),
            "R" | "R-Analog" => Some((Axis::TriggerRight, false, Scancode::E)),
            _ => None,
        }
    } else {
        None
    };
    if let Some((axis, negative, key)) = axis {
        let key = if matches!(group, "Main Stick" | "Stick" | "Left Stick" | "Tilt") {
            match control {
                "Up" | "Forward" => Scancode::W,
                "Down" | "Backward" => Scancode::S,
                "Left" => Scancode::A,
                "Right" => Scancode::D,
                _ => key,
            }
        } else {
            key
        };
        result.keyboard = Some(Physical::Key(key.name().into()));
        result.gamepad = Some(Physical::Axis {
            name: axis.string(),
            negative,
        });
    }
    if group == "Shake" && control == "X" && system == "Wii" {
        result.keyboard = Some(Physical::Key(Scancode::Space.name().into()));
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn per_game_bindings_survive_reopen_and_clear_held_inputs() {
        let dir =
            std::env::temp_dir().join(format!("emulia-dolphin-controls-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("controls.json");
        let mut controls = Controls::open(&path).unwrap();
        controls.config.style = 3;
        controls.config.maps.insert(
            "Classic/Buttons/A".into(),
            Binding {
                keyboard: Some(Physical::Key("Q".into())),
                gamepad: None,
            },
        );
        controls.save().unwrap();
        let mut controls = Controls::open(&path).unwrap();
        assert_eq!(controls.config.style, 3);
        controls.keys.insert("Q".into());
        assert_eq!(controls.value("Classic/Buttons/A"), 1.0);
        controls.clear();
        assert_eq!(controls.value("Classic/Buttons/A"), 0.0);
        std::fs::write(&path, b"invalid").unwrap();
        assert!(Controls::open(&path).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn pointer_and_two_sticks_have_independent_default_axes() {
        let main = default_binding("GC/Main Stick/Right");
        let second = default_binding("GC/C-Stick/Right");
        assert_ne!(main.gamepad, second.gamepad);
        assert_ne!(main.keyboard, second.keyboard);
        assert_eq!(default_binding("Wii/IR/Right").gamepad, second.gamepad);
    }
    #[test]
    fn mouse_aim_clicks_bounds_and_pause_are_independent_of_key_bindings() {
        let mut controls = Controls::open(Path::new("/tmp/emulia-no-mouse-config.json")).unwrap();
        controls.pointer([0.5, -0.25], 0, true);
        assert_eq!(controls.value("Wii/IR/Right"), 0.5);
        assert_eq!(controls.value("Wii/IR/Down"), 0.25);
        assert_eq!(controls.value("Wii/IR/Left"), 0.0);
        controls.qualify_input("axis", "rightx", 20000);
        controls.pointer([0.5, -0.25], 0, true);
        assert_ne!(
            controls.value("Wii/IR/Right"),
            0.5,
            "stick takes over from a stationary mouse"
        );
        controls.pointer([0.75, -0.25], 0, true);
        assert_eq!(
            controls.value("Wii/IR/Right"),
            0.75,
            "moving the mouse takes aim back"
        );
        controls.pointer([0.5, -0.25], 1, true);
        assert_eq!(controls.value("Wii/Buttons/A"), 1.0);
        controls.pointer([0.0, 0.0], 1, false);
        assert_eq!(controls.value("Wii/Buttons/A"), 0.0);
        assert_eq!(controls.value("Wii/IR/Hide"), 1.0);
        controls.clear();
        controls.pointer([0.0, 0.0], 1, true);
        assert_eq!(
            controls.value("Wii/Buttons/A"),
            0.0,
            "held menu click must not reach the game"
        );
        controls.pointer([0.0, 0.0], 0, true);
        controls.pointer([0.0, 0.0], 1, true);
        assert_eq!(controls.value("Wii/Buttons/A"), 1.0);
        controls.pointer([0.0, 0.0], 2, true);
        assert_eq!(controls.value("Wii/Buttons/B"), 1.0);
        assert_eq!(controls.value("Classic/Buttons/B"), 1.0);
        controls.config.mouse = false;
        assert_eq!(controls.value("Wii/Buttons/B"), 0.0);
    }
    #[test]
    fn nunchuk_stick_does_not_press_c_or_tilt_the_remote() {
        let mut controls =
            Controls::open(Path::new("/tmp/emulia-no-controls-config.json")).unwrap();
        controls.config.style = 2;
        controls.keys.insert("S".into());
        assert_eq!(controls.value("Nunchuk/Stick/Down"), 1.0);
        assert_eq!(controls.value("Nunchuk/Buttons/C"), 0.0);
        controls.axes.insert(Axis::LeftX.string(), 20000);
        assert_eq!(controls.value("Wii/Tilt/Right"), 0.0);
        controls.config.style = 1;
        assert!(controls.value("Wii/Tilt/Right") > 0.0);
    }
    #[test]
    fn analog_deadzone_retains_range_and_direction() {
        assert_eq!(axis_value(3000, false), 0.0);
        assert_eq!(axis_value(i16::MIN, true), 1.0);
        assert_eq!(axis_value(20000, true), 0.0);
        assert!((0.4..0.7).contains(&axis_value(20000, false)));
    }
    #[test]
    fn clearing_differs_from_missing_default() {
        assert!(default_binding("GC/Buttons/A").keyboard.is_some());
        let mut config = Config::default();
        config
            .maps
            .insert("GC/Buttons/A".into(), Binding::default());
        let loaded: Config = serde_json::from_slice(&serde_json::to_vec(&config).unwrap()).unwrap();
        assert!(loaded.maps["GC/Buttons/A"].keyboard.is_none());
    }
}
