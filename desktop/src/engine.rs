//! Core selection around the desktop shell's original NES implementation.
mod nes;
mod snes;

use emulation_api::{Buttons, PixelFormat, Presentation, Rect, VideoDescriptor};

pub enum Engine {
    Nes(Box<nes::Engine>),
    Snes(Box<snes::Engine>),
}

impl Engine {
    pub fn new(rom: &[u8]) -> Result<Self, String> {
        if rom.starts_with(b"NES\x1a") {
            nes::Engine::new(rom).map(|core| Self::Nes(Box::new(core)))
        } else {
            snes::Engine::new(rom).map(|core| Self::Snes(Box::new(core)))
        }
    }
    pub fn is_snes(&self) -> bool {
        matches!(self, Self::Snes(_))
    }
    pub fn id(&self) -> String {
        match self {
            Self::Nes(n) => n.id(),
            Self::Snes(s) => s.id(),
        }
    }
    pub fn identity(&self) -> String {
        match self {
            Self::Nes(n) => n.identity(),
            Self::Snes(s) => s.id(),
        }
    }
    pub fn frame_rate(&self) -> f64 {
        match self {
            Self::Nes(n) => n.frame_rate(),
            Self::Snes(s) => s.frame_rate(),
        }
    }
    pub fn frames_for(&self, seconds: u32) -> usize {
        match self {
            Self::Nes(n) => n.frames_for(seconds),
            Self::Snes(_) => (seconds as f64 * self.frame_rate()) as usize,
        }
    }
    pub fn header_notes(&self) -> Vec<&'static str> {
        match self {
            Self::Nes(n) => n.header_notes(),
            Self::Snes(_) => vec![],
        }
    }
    pub fn set_palette(&mut self, colours: [u32; 64]) {
        if let Self::Nes(n) = self {
            n.set_palette(colours);
        }
    }
    pub fn frame(&self) -> &[u8] {
        match self {
            Self::Nes(n) => n.frame(),
            Self::Snes(s) => s.frame(),
        }
    }
    pub fn video_descriptor(&self) -> VideoDescriptor {
        match self {
            Self::Snes(s) => s.video_descriptor(),
            Self::Nes(_) => nes_video_descriptor(),
        }
    }
    pub fn step(&mut self, p1: Buttons, p2: Buttons) -> Result<(), String> {
        match self {
            Self::Nes(n) => {
                n.step(nes_core::Buttons(p1.0 as u8), nes_core::Buttons(p2.0 as u8));
                Ok(())
            }
            Self::Snes(s) => s.step(p1, p2),
        }
    }
    pub fn samples(&self) -> &[f32] {
        match self {
            Self::Nes(n) => n.samples(),
            Self::Snes(s) => s.samples(),
        }
    }
    pub fn channels(&self) -> u8 {
        if self.is_snes() {
            2
        } else {
            1
        }
    }
    pub fn rewind_step(&mut self) -> Result<bool, String> {
        match self {
            Self::Nes(n) => Ok(n.rewind_step()),
            Self::Snes(s) => s.rewind_step(),
        }
    }
    pub fn rewind_depth(&self) -> usize {
        match self {
            Self::Nes(n) => n.rewind_depth(),
            Self::Snes(s) => s.rewind_depth(),
        }
    }
    pub fn save_state(&self) -> Result<Vec<u8>, String> {
        match self {
            Self::Nes(n) => Ok(n.save_state()),
            Self::Snes(s) => s.save_state(),
        }
    }
    pub fn load_state(&mut self, bytes: &[u8]) -> Result<(), String> {
        match self {
            Self::Nes(n) => n.load_state(bytes),
            Self::Snes(s) => s.load_state(bytes),
        }
    }
    pub fn battery_ram(&self) -> Option<&[u8]> {
        match self {
            Self::Nes(n) => n.battery_ram(),
            Self::Snes(s) => s.battery_ram(),
        }
    }
    pub fn load_battery(&mut self, bytes: &[u8]) -> Result<(), String> {
        match self {
            Self::Nes(n) => n.load_battery(bytes),
            Self::Snes(s) => s.load_battery(bytes),
        }
    }
    pub fn reset(&mut self) -> Result<(), String> {
        match self {
            Self::Nes(n) => n.reset(),
            Self::Snes(s) => s.reset(),
        }
    }
}

pub fn nes_video_descriptor() -> VideoDescriptor {
    VideoDescriptor {
        width: 256,
        height: 240,
        stride_bytes: 1024,
        visible: Rect {
            x: 0,
            y: 0,
            width: 256,
            height: 240,
        },
        pixel_aspect: (8, 7),
        format: PixelFormat::Rgba8888,
        presentation: Presentation::Progressive,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    pub use super::nes::tests::test_rom;
    pub fn snes_rom(battery: bool) -> Vec<u8> {
        let mut bytes = vec![0; 32768];
        // Change CGRAM and battery SRAM every loop, without an enhancement chip.
        let program = [
            0x78, 0xa9, 0x0f, 0x8d, 0x00, 0x21, 0xe6, 0x00, 0xa5, 0x00, 0x9c, 0x21, 0x21, 0x8d,
            0x22, 0x21, 0x9c, 0x22, 0x21, 0xa9, 0xa7, 0x8f, 0x00, 0x00, 0x70, 0x80, 0xeb,
        ];
        bytes[..program.len()].copy_from_slice(&program);
        bytes[0x7fc0..0x7fd5].copy_from_slice(b"EMULIA GENERATED TEST");
        bytes[0x7fd5] = 0x20;
        bytes[0x7fd6] = if battery { 2 } else { 0 };
        bytes[0x7fd7] = 5;
        bytes[0x7fd8] = u8::from(battery);
        bytes[0x7fd9] = 1;
        bytes[0x7fdc] = 0xff;
        bytes[0x7fdd] = 0xff;
        bytes[0x7ffd] = 0x80;
        bytes
    }
}
