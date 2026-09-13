//! Who is holding what. The keyboard and the first controller share port 1,
//! the second controller is port 2, and a saved profile beats the built-in
//! layout, which is the whole point of the mapping wizard.

use crate::bridge::key_name;
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
    Up,
    Down,
    Left,
    Right,
    X,
    Y,
    L,
    R,
}

impl NesButton {
    const SNES_FACE: [Self; 8] = [
        Self::A,
        Self::B,
        Self::X,
        Self::Y,
        Self::L,
        Self::R,
        Self::Select,
        Self::Start,
    ];
    const SNES_ORDER: [Self; 12] = [
        Self::A,
        Self::B,
        Self::X,
        Self::Y,
        Self::L,
        Self::R,
        Self::Select,
        Self::Start,
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
    ];
    /// Every button the shell can be asked to learn, buttons first and then
    /// directions, because the first four are all a controller is asked for:
    /// `FACE` is that prefix, and the wizard hands out one or the other.
    pub const ORDER: [NesButton; 8] = [
        NesButton::A,
        NesButton::B,
        NesButton::Select,
        NesButton::Start,
        NesButton::Up,
        NesButton::Down,
        NesButton::Left,
        NesButton::Right,
    ];
    pub const FACE: [NesButton; 4] = [
        NesButton::A,
        NesButton::B,
        NesButton::Select,
        NesButton::Start,
    ];
    pub fn bit(self) -> u16 {
        match self {
            NesButton::A => 1,
            NesButton::B => 2,
            NesButton::Select => 4,
            NesButton::Start => 8,
            NesButton::Up => 16,
            NesButton::Down => 32,
            NesButton::Left => 64,
            NesButton::Right => 128,
            NesButton::X => 256,
            NesButton::Y => 512,
            NesButton::L => 1024,
            NesButton::R => 2048,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            NesButton::A => "A",
            NesButton::B => "B",
            NesButton::Select => "Select",
            NesButton::Start => "Start",
            NesButton::Up => "Up",
            NesButton::Down => "Down",
            NesButton::Left => "Left",
            NesButton::Right => "Right",
            NesButton::X => "X",
            NesButton::Y => "Y",
            NesButton::L => "L",
            NesButton::R => "R",
        }
    }
    /// The name a profile has saved for this button, empty when it has none.
    fn saved(self, profile: &Profile) -> &str {
        match self {
            NesButton::A => &profile.a,
            NesButton::B => &profile.b,
            NesButton::Select => &profile.select,
            NesButton::Start => &profile.start,
            NesButton::Up => &profile.up,
            NesButton::Down => &profile.down,
            NesButton::Left => &profile.left,
            NesButton::Right => &profile.right,
            NesButton::X => &profile.snes[0],
            NesButton::Y => &profile.snes[1],
            NesButton::L => &profile.snes[2],
            NesButton::R => &profile.snes[3],
        }
    }
}

/// The arrow key each direction falls back to when a profile does not name
/// one. A profile written before the directions could be remapped has all
/// four empty, and this is what keeps it steering by the arrows.
const ARROWS: [(NesButton, Scancode); 4] = [
    (NesButton::Up, Scancode::Up),
    (NesButton::Down, Scancode::Down),
    (NesButton::Left, Scancode::Left),
    (NesButton::Right, Scancode::Right),
];

/// Whether an axis has actually been moved: past the stick's dead zone either
/// way, or past the point a trigger counts as pulled. A resting stick reports
/// small numbers for ever, and taking those for a sign of life would hold the
/// full-screen chrome up whether or not anybody was there.
pub fn past_dead_zone(axis: Axis, value: i16) -> bool {
    match axis {
        Axis::TriggerLeft | Axis::TriggerRight => value > TRIGGER_ON,
        _ => value.saturating_abs() > STICK_DEAD_ZONE,
    }
}

pub fn keyboard_default() -> Profile {
    Profile {
        name: "Keyboard".into(),
        a: "X".into(),
        b: "Z".into(),
        select: "Right Shift".into(),
        start: "Return".into(),
        // Spelled through the same function the wizard saves a captured key
        // with, so a default and a mapping somebody made mean the same thing.
        up: key_name(Scancode::Up),
        down: key_name(Scancode::Down),
        left: key_name(Scancode::Left),
        right: key_name(Scancode::Right),
        ..Profile::default()
    }
}

/// SDL names the south face button "a" and the east one "b". NES A is the
/// right-hand button and B the left, and a thumb rests on the south button,
/// so it drives NES B. The other way round makes every game feel backwards.
pub fn controller_default(name: &str) -> Profile {
    Profile {
        name: name.into(),
        a: "b".into(),
        b: "a".into(),
        select: "back".into(),
        start: "start".into(),
        // A pad steers from its d-pad and its left stick, which are read
        // straight off the hardware; there is nothing here to name.
        ..Profile::default()
    }
}

fn physical_bits(profile: &Profile, held: impl Fn(&str) -> bool) -> u8 {
    let mut bits = 0;
    for button in NesButton::FACE {
        if held(button.saved(profile)) {
            bits |= button.bit() as u8;
        }
    }
    bits
}

pub fn keyboard_bits(profile: &Profile, keys: &HashSet<Scancode>) -> u8 {
    let held = |name: &str| Scancode::from_name(name).is_some_and(|s| keys.contains(&s));
    let mut bits = physical_bits(profile, held);
    for (button, arrow) in ARROWS {
        let name = button.saved(profile);
        // An unnamed direction is the arrow key's, and a named one is only
        // the key it names: the arrow does not stay on as a second way in,
        // or WASD would leave the arrows steering as well.
        let down = if name.is_empty() {
            keys.contains(&arrow)
        } else {
            held(name)
        };
        if down {
            bits |= button.bit() as u8;
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
    // Bumpers and triggers are read separately: a trigger settling back to
    // rest must not cancel a bumper the other finger is still holding.
    shoulder_time: i32,
    trigger_time: i32,
}

pub struct Input {
    profiles: Profiles,
    keys: HashSet<Scancode>,
    pads: Vec<Pad>,
    dirty: bool,
}

impl Input {
    pub fn new(profiles: Profiles) -> Input {
        Input {
            profiles,
            keys: HashSet::new(),
            pads: Vec::new(),
            dirty: false,
        }
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
            pad.shoulder_time = 0;
            pad.trigger_time = 0;
        }
    }

    pub fn pad_added(&mut self, controller: GameController, guid: String) -> Option<usize> {
        let instance = controller.instance_id();
        if self.pads.iter().any(|p| p.instance == instance) {
            return None;
        }
        let port = (0..2).find(|port| !self.pads.iter().any(|p| p.port == *port))?;
        let name = controller.name();
        self.pads.push(Pad {
            instance,
            guid,
            name,
            port,
            _controller: controller,
            pressed: HashSet::new(),
            dpad: 0,
            stick: 0,
            shoulder_time: 0,
            trigger_time: 0,
        });
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
            if pressed {
                pad.dpad |= dpad
            } else {
                pad.dpad &= !dpad
            }
            return;
        }
        let time = match button {
            Button::RightShoulder => TIME_SLOW,
            Button::LeftShoulder => -TIME_SLOW,
            _ => 0,
        };
        if time != 0 {
            pad.shoulder_time = if pressed { time } else { 0 };
        }
        if pressed {
            pad.pressed.insert(name);
        } else {
            pad.pressed.remove(&name);
        }
    }

    pub fn pad_axis(&mut self, instance: u32, axis: Axis, value: i16) {
        let Some(pad) = self.pads.iter_mut().find(|p| p.instance == instance) else {
            return;
        };
        match axis {
            Axis::LeftX => {
                pad.stick &= !(64 | 128);
                if value < -STICK_DEAD_ZONE {
                    pad.stick |= 64
                } else if value > STICK_DEAD_ZONE {
                    pad.stick |= 128
                }
            }
            Axis::LeftY => {
                pad.stick &= !(16 | 32);
                if value < -STICK_DEAD_ZONE {
                    pad.stick |= 16
                } else if value > STICK_DEAD_ZONE {
                    pad.stick |= 32
                }
            }
            Axis::TriggerRight => pad.trigger_time = if value > TRIGGER_ON { TIME_FAST } else { 0 },
            Axis::TriggerLeft => pad.trigger_time = if value > TRIGGER_ON { -TIME_FAST } else { 0 },
            _ => {}
        }
    }

    fn keyboard_profile(&self) -> Profile {
        self.profiles
            .get(Profiles::KEYBOARD)
            .cloned()
            .unwrap_or_else(keyboard_default)
    }

    fn pad_profile(&self, pad: &Pad) -> Profile {
        self.profiles
            .get(&pad.guid)
            .cloned()
            .unwrap_or_else(|| controller_default(&pad.name))
    }

    pub fn buttons(&self) -> (Buttons, Buttons) {
        let mut ports = [keyboard_bits(&self.keyboard_profile(), &self.keys), 0];
        for pad in &self.pads {
            ports[pad.port] |= pad_bits(&self.pad_profile(pad), &pad.pressed, pad.dpad | pad.stick);
        }
        (Buttons(ports[0]), Buttons(ports[1]))
    }

    fn snes_profile(&self, key: &str, name: &str) -> Profile {
        self.profiles
            .get(&format!("snes:{key}"))
            .cloned()
            .unwrap_or_else(|| {
                let mut profile = if key == Profiles::KEYBOARD {
                    keyboard_default()
                } else {
                    controller_default(name)
                };
                profile.snes = if key == Profiles::KEYBOARD {
                    ["S", "A", "Q", "W"]
                } else {
                    ["y", "x", "leftshoulder", "rightshoulder"]
                }
                .map(String::from);
                profile
            })
    }

    pub fn game_buttons(&self, snes: bool) -> (emulation_api::Buttons, emulation_api::Buttons) {
        use emulation_api::Buttons as B;
        if !snes {
            let (p1, p2) = self.buttons();
            return (B(p1.0 as u16), B(p2.0 as u16));
        }
        let keyboard = self.snes_profile(Profiles::KEYBOARD, "Keyboard");
        let mut ports = [keyboard_bits(&keyboard, &self.keys) as u16, 0];
        for button in [NesButton::X, NesButton::Y, NesButton::L, NesButton::R] {
            if Scancode::from_name(button.saved(&keyboard))
                .is_some_and(|key| self.keys.contains(&key))
            {
                ports[0] |= button.bit();
            }
        }
        for pad in &self.pads {
            let profile = self.snes_profile(&pad.guid, &pad.name);
            ports[pad.port] |= pad_bits(&profile, &pad.pressed, pad.dpad | pad.stick) as u16;
            for button in [NesButton::X, NesButton::Y, NesButton::L, NesButton::R] {
                if pad.pressed.contains(button.saved(&profile)) {
                    ports[pad.port] |= button.bit();
                }
            }
        }
        (B(ports[0]), B(ports[1]))
    }

    /// Shoulders and triggers, or `,` and `.` with Shift, held to apply.
    ///
    /// A pulled trigger wins over a held bumper, because the trigger is the
    /// deliberate "faster"; releasing either leaves the other still holding.
    #[cfg(test)]
    pub fn time_speed(&self) -> i32 {
        self.game_time_speed(false)
    }

    pub fn game_time_speed(&self, snes: bool) -> i32 {
        let held = |time: fn(&Pad) -> i32| self.pads.iter().map(time).find(|&t| t != 0);
        if let Some(speed) = held(|p| p.trigger_time).or_else(|| {
            if snes {
                None
            } else {
                held(|p| p.shoulder_time)
            }
        }) {
            return speed;
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
    #[cfg(test)]
    pub fn is_start(&self, instance: Option<u32>, physical: &str) -> bool {
        self.game_is_start(instance, physical, false)
    }

    pub fn game_is_start(&self, instance: Option<u32>, physical: &str, snes: bool) -> bool {
        if snes {
            let (key, name) = instance
                .and_then(|i| self.pad(i))
                .map(|pad| (pad.guid.as_str(), pad.name.as_str()))
                .unwrap_or((Profiles::KEYBOARD, "Keyboard"));
            return self.snes_profile(key, name).start == physical;
        }
        let profile = match instance.and_then(|i| self.pad(i)) {
            Some(pad) => self.pad_profile(pad),
            None => self.keyboard_profile(),
        };
        profile.start == physical
    }
}

/// The finished profile is boxed: nine strings is a large thing to carry in
/// an enum whose other two arms are empty, and every press but the last
/// returns one of those.
#[derive(Debug, PartialEq, Eq)]
pub enum WizardEvent {
    Advanced,
    Rejected,
    Done(String, Box<Profile>),
}

/// A, B, Select and Start from the device that pressed first, each a button
/// not already used — and, when that device is the keyboard, the four
/// directions after them, because a keyboard has no d-pad to fall back on.
#[derive(Default)]
pub struct Wizard {
    snes: bool,
    device: Option<(String, String)>,
    captured: Vec<(NesButton, String)>,
}

impl Wizard {
    pub fn new() -> Wizard {
        Wizard::default()
    }
    pub fn for_system(snes: bool) -> Self {
        Self {
            snes,
            ..Self::new()
        }
    }
    pub fn step(&self) -> usize {
        self.captured.len()
    }
    pub fn device_name(&self) -> Option<&str> {
        self.device.as_ref().map(|d| d.1.as_str())
    }
    /// What this wizard will ask for, in order. Until something presses there
    /// is no device, and the four buttons every mapping starts with are all
    /// that can be said for certain; the keyboard adds its directions to them
    /// on its first key.
    pub fn steps(&self) -> &'static [NesButton] {
        if self.snes {
            return match &self.device {
                Some((key, _)) if key == Profiles::KEYBOARD => &NesButton::SNES_ORDER,
                _ => &NesButton::SNES_FACE,
            };
        }
        match &self.device {
            Some((key, _)) if key == Profiles::KEYBOARD => &NesButton::ORDER,
            _ => &NesButton::FACE,
        }
    }
    /// How many steps there are, once the device that pressed first has said
    /// so. `None` before then: a total nobody can know yet is one the panel
    /// would have to take back.
    pub fn total(&self) -> Option<usize> {
        self.device.as_ref().map(|_| self.steps().len())
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
        let steps = self.steps();
        // Two buttons hit together at the last step arrive as two events, so
        // a press past the end is refused rather than indexed for.
        let Some(&button) = steps.get(self.captured.len()) else {
            return WizardEvent::Rejected;
        };
        self.captured.push((button, physical.to_string()));
        if self.captured.len() < steps.len() {
            return WizardEvent::Advanced;
        }
        // A button the wizard never asked about is left empty, which for a
        // controller is all four directions: its d-pad and stick steer on
        // their own, and an empty direction is the arrow key's for a keyboard.
        let find = |b: NesButton| {
            self.captured
                .iter()
                .find(|(x, _)| *x == b)
                .map(|(_, p)| p.clone())
                .unwrap_or_default()
        };
        WizardEvent::Done(
            if self.snes {
                format!("snes:{device_key}")
            } else {
                device_key.to_string()
            },
            Box::new(Profile {
                snes: [NesButton::X, NesButton::Y, NesButton::L, NesButton::R].map(find),
                name: device_name.to_string(),
                a: find(NesButton::A),
                b: find(NesButton::B),
                select: find(NesButton::Select),
                start: find(NesButton::Start),
                up: find(NesButton::Up),
                down: find(NesButton::Down),
                left: find(NesButton::Left),
                right: find(NesButton::Right),
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stick pushed or a trigger pulled is somebody at the controls, and a
    /// stick lying still is not. The full-screen chrome fades on this: read
    /// the resting jitter as a sign of life and it never fades at all; ignore
    /// a held direction and it fades under the thumb holding it.
    #[test]
    fn snes_shoulders_are_gameplay_and_triggers_still_control_time() {
        use emulation_api::Buttons as B;
        let sdl = sdl2::init().unwrap();
        let controllers = sdl.game_controller().unwrap();
        // SAFETY: SDL's controller subsystem is alive; no pointers cross this call.
        let index = unsafe {
            sdl2::sys::SDL_JoystickAttachVirtual(
                sdl2::sys::SDL_JoystickType::SDL_JOYSTICK_TYPE_GAMECONTROLLER,
                6,
                21,
                1,
            )
        };
        assert!(index >= 0, "{}", sdl2::get_error());
        let controller = controllers.open(index as u32).unwrap();
        let instance = controller.instance_id();
        let mut input = Input::new(Profiles::default());
        input.pad_added(controller, "test-virtual".into());
        input.pad_button(instance, Button::LeftShoulder, true);
        input.pad_button(instance, Button::RightShoulder, true);
        assert_eq!(input.game_buttons(true).0 .0, B::L | B::R);
        assert_eq!(input.game_time_speed(true), 0);
        assert_ne!(input.game_time_speed(false), 0);
        input.pad_axis(instance, Axis::TriggerRight, i16::MAX);
        assert_eq!(input.game_time_speed(true), TIME_FAST);
        assert_eq!(input.game_buttons(true).0 .0, B::L | B::R);
        input.pad_button(instance, Button::LeftShoulder, false);
        assert_eq!(input.game_buttons(true).0 .0, B::R);
        input.clear();
        assert_eq!(input.game_buttons(true).0 .0, 0);
        assert_eq!(input.game_time_speed(true), 0);
        drop(input);
        // SAFETY: the virtual controller was closed before detaching its device.
        assert_eq!(unsafe { sdl2::sys::SDL_JoystickDetachVirtual(index) }, 0);
    }

    #[test]
    fn snes_mapping_is_separate_and_includes_all_twelve_keys() {
        use emulation_api::Buttons as B;
        let mut input = Input::new(Profiles::default());
        for key in [Scancode::A, Scancode::S, Scancode::Q, Scancode::W] {
            input.key(key, true);
        }
        assert_eq!(input.game_buttons(true).0 .0, B::X | B::Y | B::L | B::R);
        assert_eq!(input.game_buttons(false).0 .0, 0);
        input.clear();
        let mut wizard = Wizard::for_system(true);
        let keys = [
            "K", "J", "I", "U", "H", "L", "Tab", "Return", "Up", "Down", "Left", "Right",
        ];
        for (i, key) in keys.iter().enumerate() {
            let event = wizard.press(Profiles::KEYBOARD, "Keyboard", key);
            if i == keys.len() - 1 {
                let WizardEvent::Done(key, profile) = event else {
                    panic!("wizard did not complete");
                };
                assert_eq!(key, "snes:keyboard");
                input.set_profile(key, *profile);
            } else {
                assert_eq!(event, WizardEvent::Advanced);
            }
        }
        input.key(Scancode::U, true);
        input.key(Scancode::H, true);
        assert_eq!(input.game_buttons(true).0 .0, B::Y | B::L);
        assert_eq!(input.game_buttons(false).0 .0, 0);
        input.key(Scancode::X, true);
        assert_eq!(input.game_buttons(false).0 .0, B::A);
        assert_eq!(input.game_buttons(true).0 .0, B::Y | B::L);
    }

    #[test]
    fn a_stick_at_rest_is_not_somebody_at_the_controls() {
        for axis in [Axis::LeftX, Axis::LeftY, Axis::RightX, Axis::RightY] {
            assert!(!past_dead_zone(axis, 0), "{axis:?}");
            assert!(!past_dead_zone(axis, STICK_DEAD_ZONE), "{axis:?}");
            assert!(!past_dead_zone(axis, -STICK_DEAD_ZONE), "{axis:?}");
            assert!(past_dead_zone(axis, STICK_DEAD_ZONE + 1), "{axis:?}");
            assert!(past_dead_zone(axis, i16::MIN), "{axis:?}");
            assert!(past_dead_zone(axis, i16::MAX), "{axis:?}");
        }
        // A trigger only goes one way, and it counts from further down.
        for axis in [Axis::TriggerLeft, Axis::TriggerRight] {
            assert!(!past_dead_zone(axis, TRIGGER_ON), "{axis:?}");
            assert!(past_dead_zone(axis, TRIGGER_ON + 1), "{axis:?}");
            assert!(!past_dead_zone(axis, 0), "{axis:?}");
        }
    }

    #[test]
    fn nes_button_bits_and_order_match_the_core() {
        assert_eq!(
            NesButton::ORDER.map(|b| b.bit() as u8),
            [
                Buttons::A,
                Buttons::B,
                Buttons::SELECT,
                Buttons::START,
                Buttons::UP,
                Buttons::DOWN,
                Buttons::LEFT,
                Buttons::RIGHT
            ]
        );
        assert_eq!(
            NesButton::ORDER.map(|b| b.label()),
            ["A", "B", "Select", "Start", "Up", "Down", "Left", "Right"]
        );
        let arrows: HashSet<Scancode> = [
            Scancode::Up,
            Scancode::Down,
            Scancode::Left,
            Scancode::Right,
        ]
        .into_iter()
        .collect();
        assert_eq!(
            keyboard_bits(&keyboard_default(), &arrows),
            Buttons::UP | Buttons::DOWN | Buttons::LEFT | Buttons::RIGHT
        );
        // A pad steers from its own hardware, whatever its profile says.
        let pads: HashSet<String> = HashSet::new();
        assert_eq!(
            pad_bits(&controller_default("Pad"), &pads, Buttons::UP),
            Buttons::UP
        );
    }

    /// The keyboard's directions are a profile's to name. A profile that
    /// names them steers by them; one that leaves them empty — which is every
    /// profile written before the wizard asked for them — steers by the
    /// arrows, so nobody's saved mapping changes under them.
    #[test]
    fn keyboard_directions_come_from_the_profile_and_fall_back_to_the_arrows() {
        let wasd = Profile {
            snes: Default::default(),
            name: "Keyboard".into(),
            a: "X".into(),
            b: "Z".into(),
            select: "Right Shift".into(),
            start: "Return".into(),
            up: "W".into(),
            down: "S".into(),
            left: "A".into(),
            right: "D".into(),
        };
        let held = |keys: &[Scancode]| keys.iter().copied().collect::<HashSet<Scancode>>();
        assert_eq!(
            keyboard_bits(&wasd, &held(&[Scancode::W, Scancode::D])),
            Buttons::UP | Buttons::RIGHT
        );
        // The arrows are somebody else's keys now, not a second way in.
        assert_eq!(keyboard_bits(&wasd, &held(&[Scancode::Up])), 0);
        // A is NES left on this profile and X is NES A, both at once.
        assert_eq!(
            keyboard_bits(&wasd, &held(&[Scancode::A, Scancode::X])),
            Buttons::LEFT | Buttons::A
        );

        // The four-field shape an older shell wrote: empty is the arrow.
        let old = Profile {
            up: String::new(),
            down: String::new(),
            left: String::new(),
            right: String::new(),
            ..wasd.clone()
        };
        assert_eq!(
            keyboard_bits(&old, &held(&[Scancode::Left, Scancode::Down])),
            Buttons::LEFT | Buttons::DOWN
        );
        assert_eq!(keyboard_bits(&old, &held(&[Scancode::W])), 0);
        // And one direction named on its own leaves the other three arrows.
        let half = Profile {
            up: "W".into(),
            ..old.clone()
        };
        assert_eq!(
            keyboard_bits(&half, &held(&[Scancode::W, Scancode::Right])),
            Buttons::UP | Buttons::RIGHT
        );
        assert_eq!(keyboard_bits(&half, &held(&[Scancode::Up])), 0);

        // A direction naming a key SDL has never heard of — a profile typed
        // by hand — steers nothing at all. Falling back to the arrow would be
        // a profile that says one key and plays as another.
        let nonsense = Profile {
            up: "Wsdf".into(),
            ..old.clone()
        };
        assert_eq!(keyboard_bits(&nonsense, &held(&[Scancode::Up])), 0);
        assert_eq!(
            keyboard_bits(&nonsense, &held(&[Scancode::W, Scancode::Down])),
            Buttons::DOWN
        );
    }

    #[test]
    fn the_default_keyboard_profile_is_the_first_release_layout() {
        let p = keyboard_default();
        assert_eq!(
            (
                p.a.as_str(),
                p.b.as_str(),
                p.select.as_str(),
                p.start.as_str()
            ),
            ("X", "Z", "Right Shift", "Return")
        );
        // Spelled the way the wizard spells a captured key, so a default and
        // a mapping made by hand mean the same thing to `Scancode::from_name`.
        assert_eq!(
            (
                p.up.as_str(),
                p.down.as_str(),
                p.left.as_str(),
                p.right.as_str()
            ),
            ("Up", "Down", "Left", "Right")
        );
        for name in [&p.up, &p.down, &p.left, &p.right] {
            assert!(Scancode::from_name(name).is_some(), "{name}");
        }
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
        profiles.set(
            Profiles::KEYBOARD.into(),
            Profile {
                name: "Keyboard".into(),
                a: "K".into(),
                b: "J".into(),
                select: "Tab".into(),
                start: "Space".into(),
                ..Profile::default()
            },
        );
        let mut input = Input::new(profiles);
        input.key(Scancode::K, true);
        input.key(Scancode::X, true);
        // `Buttons` is not `PartialEq`, so the held bits are compared directly.
        assert_eq!(input.buttons().0 .0, Buttons(1).0);
        assert!(input.is_start(None, "Space"));
        assert!(!input.is_start(None, "Return"));
        input.clear();
        assert_eq!(input.buttons().0 .0, Buttons(0).0);
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
    fn the_wizard_takes_four_distinct_buttons_from_one_pad() {
        let mut w = Wizard::new();
        assert_eq!(w.step(), 0);
        // Nothing has pressed yet, so there is no total to promise.
        assert_eq!(w.total(), None);
        assert_eq!(w.press("pad1", "Pad", "b"), WizardEvent::Advanced);
        assert_eq!(w.device_name(), Some("Pad"));
        assert_eq!(w.total(), Some(4));
        assert_eq!(w.press("pad2", "Other", "a"), WizardEvent::Rejected);
        assert_eq!(w.press("pad1", "Pad", "b"), WizardEvent::Rejected);
        assert_eq!(w.press("pad1", "Pad", "a"), WizardEvent::Advanced);
        assert_eq!(w.press("pad1", "Pad", "back"), WizardEvent::Advanced);
        assert_eq!(w.step(), 3);
        match w.press("pad1", "Pad", "start") {
            WizardEvent::Done(key, profile) => {
                assert_eq!(key, "pad1");
                let profile = *profile;
                assert_eq!(
                    profile,
                    Profile {
                        snes: Default::default(),
                        name: "Pad".into(),
                        a: "b".into(),
                        b: "a".into(),
                        select: "back".into(),
                        start: "start".into(),
                        ..Profile::default()
                    }
                );
                // A pad steers from its d-pad and stick, which the wizard
                // never asked about: its directions stay unnamed.
                assert_eq!(
                    (profile.up, profile.down, profile.left, profile.right),
                    (String::new(), String::new(), String::new(), String::new())
                );
            }
            other => panic!("{other:?}"),
        }
        // Two buttons at once on the last step: the extra press is refused and
        // the finished mapping stands.
        assert_eq!(w.press("pad1", "Pad", "x"), WizardEvent::Rejected);
        assert_eq!(w.step(), 4);
    }

    /// The keyboard has no d-pad to fall back on, so it is asked for all
    /// eight: the four buttons and then the four directions. Which device
    /// pressed first decides how long the wizard is.
    #[test]
    fn the_wizard_takes_eight_distinct_keys_from_the_keyboard() {
        let mut w = Wizard::new();
        assert_eq!(w.total(), None);
        let key = Profiles::KEYBOARD;
        for (i, physical) in ["X", "Z", "Right Shift", "Return", "W", "S", "A"]
            .iter()
            .enumerate()
        {
            assert_eq!(w.press(key, "Keyboard", physical), WizardEvent::Advanced);
            assert_eq!(w.total(), Some(8), "after {physical}");
            assert_eq!(w.step(), i + 1);
        }
        // A key already spoken for is refused at every one of the eight steps,
        // not just among the first four: W is up, and cannot also be right.
        assert_eq!(w.press(key, "Keyboard", "W"), WizardEvent::Rejected);
        assert_eq!(w.press(key, "Keyboard", "X"), WizardEvent::Rejected);
        // And the device that pressed first still owns the rest of the steps.
        assert_eq!(w.press("pad1", "Pad", "b"), WizardEvent::Rejected);
        assert_eq!(w.step(), 7);
        match w.press(key, "Keyboard", "D") {
            WizardEvent::Done(saved, profile) => {
                assert_eq!(saved, Profiles::KEYBOARD);
                assert_eq!(
                    *profile,
                    Profile {
                        snes: Default::default(),
                        name: "Keyboard".into(),
                        a: "X".into(),
                        b: "Z".into(),
                        select: "Right Shift".into(),
                        start: "Return".into(),
                        up: "W".into(),
                        down: "S".into(),
                        left: "A".into(),
                        right: "D".into(),
                    }
                );
            }
            other => panic!("{other:?}"),
        }
        // Two keys at once on the last step: the extra press is refused.
        assert_eq!(w.press(key, "Keyboard", "Q"), WizardEvent::Rejected);
        assert_eq!(w.step(), 8);
    }

    /// What the panel names at each step, before and after the device is
    /// known. Until something presses there is no device and so no total:
    /// the four buttons every mapping starts with are all the wizard can
    /// promise, and the keyboard adds its four directions on the first press.
    #[test]
    fn the_step_list_follows_the_device_that_pressed_first() {
        let base = [
            NesButton::A,
            NesButton::B,
            NesButton::Select,
            NesButton::Start,
        ];
        let w = Wizard::new();
        assert_eq!(w.steps(), base);
        let mut pad = Wizard::new();
        pad.press("pad1", "Pad", "b");
        assert_eq!(pad.steps(), base);
        let mut keyboard = Wizard::new();
        keyboard.press(Profiles::KEYBOARD, "Keyboard", "X");
        assert_eq!(keyboard.steps(), NesButton::ORDER);
    }
}
