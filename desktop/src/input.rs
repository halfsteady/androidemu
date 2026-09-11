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
    pub const ORDER: [NesButton; 4] = [
        NesButton::A,
        NesButton::B,
        NesButton::Select,
        NesButton::Start,
    ];
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
    Profile {
        name: "Keyboard".into(),
        a: "X".into(),
        b: "Z".into(),
        select: "Right Shift".into(),
        start: "Return".into(),
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
    }
}

fn physical_bits(profile: &Profile, held: impl Fn(&str) -> bool) -> u8 {
    let mut bits = 0;
    for (button, name) in [
        (NesButton::A, &profile.a),
        (NesButton::B, &profile.b),
        (NesButton::Select, &profile.select),
        (NesButton::Start, &profile.start),
    ] {
        if held(name) {
            bits |= button.bit();
        }
    }
    bits
}

pub fn keyboard_bits(profile: &Profile, keys: &HashSet<Scancode>) -> u8 {
    let mut bits = physical_bits(profile, |name| {
        Scancode::from_name(name).is_some_and(|s| keys.contains(&s))
    });
    for (key, bit) in [
        (Scancode::Up, 16),
        (Scancode::Down, 32),
        (Scancode::Left, 64),
        (Scancode::Right, 128),
    ] {
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
            return;
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

    /// Shoulders and triggers, or `,` and `.` with Shift, held to apply.
    ///
    /// A pulled trigger wins over a held bumper, because the trigger is the
    /// deliberate "faster"; releasing either leaves the other still holding.
    pub fn time_speed(&self) -> i32 {
        let held = |time: fn(&Pad) -> i32| self.pads.iter().map(time).find(|&t| t != 0);
        if let Some(speed) = held(|p| p.trigger_time).or_else(|| held(|p| p.shoulder_time)) {
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
        // Two buttons hit together at the last step arrive as two events, so
        // a press after the fourth is refused rather than indexed for.
        let Some(&button) = NesButton::ORDER.get(self.captured.len()) else {
            return WizardEvent::Rejected;
        };
        self.captured.push((button, physical.to_string()));
        if self.captured.len() < 4 {
            return WizardEvent::Advanced;
        }
        let find = |b: NesButton| {
            self.captured
                .iter()
                .find(|(x, _)| *x == b)
                .map(|(_, p)| p.clone())
                .unwrap_or_default()
        };
        WizardEvent::Done(
            device_key.to_string(),
            Profile {
                name: device_name.to_string(),
                a: find(NesButton::A),
                b: find(NesButton::B),
                select: find(NesButton::Select),
                start: find(NesButton::Start),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nes_button_bits_and_order_match_the_core() {
        assert_eq!(
            NesButton::ORDER.map(|b| b.bit()),
            [Buttons::A, Buttons::B, Buttons::SELECT, Buttons::START]
        );
        assert_eq!(
            NesButton::ORDER.map(|b| b.label()),
            ["A", "B", "Select", "Start"]
        );
        assert_eq!(
            [Buttons::UP, Buttons::DOWN, Buttons::LEFT, Buttons::RIGHT],
            [16, 32, 64, 128]
        );
        // The directions are not part of a profile, so the bits this module
        // writes for them are checked against the core through real code.
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
        let pads: HashSet<String> = HashSet::new();
        assert_eq!(
            pad_bits(&controller_default("Pad"), &pads, Buttons::UP),
            Buttons::UP
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
                assert_eq!(
                    profile,
                    Profile {
                        name: "Pad".into(),
                        a: "b".into(),
                        b: "a".into(),
                        select: "back".into(),
                        start: "start".into()
                    }
                );
            }
            other => panic!("{other:?}"),
        }
        // Two buttons at once on the last step: the extra press is refused and
        // the finished mapping stands.
        assert_eq!(w.press("pad1", "Pad", "x"), WizardEvent::Rejected);
        assert_eq!(w.step(), 4);
    }
}
