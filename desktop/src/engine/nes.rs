//! The machine, its rewind chain and the colours its framebuffer means, with
//! nothing about windows, audio devices or files. The counterpart of the JNI
//! layer in native/src/lib.rs, and shaped the same way on purpose.

use crate::picture::{HEIGHT, WIDTH};
use nes_core::{rewind::Rewind, Buttons, Nes};

/// Bounded in bytes rather than frames: see `nes_core::rewind`.
pub const REWIND_BUDGET: usize = 64 * 1024 * 1024;

pub struct Engine {
    pub(crate) nes: Nes,
    rewind: Rewind,
    rom: Vec<u8>,
    palette: [u32; 64],
    frame: Vec<u8>,
}

impl Engine {
    pub fn new(rom: &[u8]) -> Result<Engine, String> {
        let nes = Nes::new(rom).map_err(|e| e.to_string())?;
        let mut engine = Engine {
            nes,
            rewind: Rewind::new(REWIND_BUDGET),
            rom: rom.to_vec(),
            palette: crate::palette::PALETTE,
            frame: vec![0; WIDTH * HEIGHT * 4],
        };
        engine.anchor();
        engine.repaint();
        Ok(engine)
    }

    pub fn id(&self) -> String {
        format!("{:016x}", self.nes.bus.cart.header.hash)
    }
    pub fn identity(&self) -> String {
        format!("{:016x}", self.nes.bus.cart.header.identity)
    }
    pub fn frame_rate(&self) -> f64 {
        self.nes.bus.cart.header.region.frame_rate()
    }
    pub fn frames_for(&self, seconds: u32) -> usize {
        (seconds as f64 * self.frame_rate()) as usize
    }
    pub fn header_notes(&self) -> Vec<&'static str> {
        self.nes.bus.cart.header.fixes.labels().collect()
    }

    fn repaint(&mut self) {
        for (i, &index) in self.nes.framebuffer().iter().enumerate() {
            let rgb = self.palette[(index & 63) as usize];
            let at = i * 4;
            self.frame[at] = (rgb >> 16) as u8;
            self.frame[at + 1] = (rgb >> 8) as u8;
            self.frame[at + 2] = rgb as u8;
            self.frame[at + 3] = 255;
        }
    }

    /// Start a fresh chain anchored on where the machine is now. A chain needs
    /// a state to measure the next one against, so the anchor costs no depth
    /// and one pop is then exactly one frame back, as on Android.
    fn anchor(&mut self) {
        self.rewind.clear();
        self.rewind.push(&self.nes.save_state());
    }

    pub fn set_palette(&mut self, colours: [u32; 64]) {
        self.palette = colours;
        self.repaint();
    }

    pub fn frame(&self) -> &[u8] {
        &self.frame
    }

    pub fn step(&mut self, p1: Buttons, p2: Buttons) {
        self.nes.set_buttons(0, p1);
        self.nes.set_buttons(1, p2);
        self.nes.step_frame();
        self.rewind.push(&self.nes.save_state());
        self.repaint();
    }

    pub fn samples(&self) -> &[f32] {
        self.nes.bus.apu.samples()
    }

    /// One frame back. False when the chain is dry; the frame is repainted either way.
    pub fn rewind_step(&mut self) -> bool {
        let Some(state) = self.rewind.pop() else {
            return false;
        };
        let restored = self.nes.load_state(state).is_ok();
        self.repaint();
        restored
    }
    pub fn rewind_depth(&self) -> usize {
        self.rewind.depth()
    }

    pub fn save_state(&self) -> Vec<u8> {
        self.nes.save_state()
    }
    /// A restored state leads to a timeline that was not played, so the chain goes.
    pub fn load_state(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.nes.load_state(bytes).map_err(|e| e.to_string())?;
        self.anchor();
        self.repaint();
        Ok(())
    }
    pub fn battery_ram(&self) -> Option<&[u8]> {
        self.nes.battery_ram()
    }
    pub fn load_battery(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.nes.load_battery_ram(bytes).map_err(|e| e.to_string())
    }

    /// A fresh machine from the same ROM with battery RAM carried across,
    /// which is what a console's reset button did.
    pub fn reset(&mut self) -> Result<(), String> {
        let mut fresh = Nes::new(&self.rom).map_err(|e| e.to_string())?;
        if let Some(battery) = self.nes.battery_ram() {
            fresh.load_battery_ram(battery).map_err(|e| e.to_string())?;
        }
        self.nes = fresh;
        self.anchor();
        self.repaint();
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// NROM with CHR RAM and battery: `INC $00; JMP $8000` style loop.
    pub fn test_rom() -> Vec<u8> {
        let mut rom = vec![0; 16 + 16384];
        rom[..4].copy_from_slice(b"NES\x1a");
        rom[4] = 1;
        rom[6] = 2;
        rom[16..22].copy_from_slice(&[0xe6, 0x00, 0x4c, 0x00, 0x80, 0xea]);
        rom[16 + 0x3ffc..16 + 0x3ffe].copy_from_slice(&[0, 0x80]);
        rom
    }

    #[test]
    fn the_id_is_the_header_hash_android_uses() {
        let engine = Engine::new(&test_rom()).unwrap();
        let nes = Nes::new(&test_rom()).unwrap();
        assert_eq!(engine.id(), format!("{:016x}", nes.bus.cart.header.hash));
        assert_eq!(
            engine.identity(),
            format!("{:016x}", nes.bus.cart.header.identity)
        );
        assert_eq!(engine.id().len(), 16);
    }

    #[test]
    fn stepping_paints_rgba_and_grows_the_rewind_chain() {
        let mut engine = Engine::new(&test_rom()).unwrap();
        assert_eq!(engine.frame().len(), 256 * 240 * 4);
        assert_eq!(engine.rewind_depth(), 0);
        engine.step(Buttons(0), Buttons(0));
        assert_eq!(engine.rewind_depth(), 1);
        assert!(!engine.samples().is_empty());
        assert!(engine.frame().iter().skip(3).step_by(4).all(|&a| a == 255));
        assert!(engine.rewind_step());
        assert_eq!(engine.rewind_depth(), 0);
        assert!(!engine.rewind_step());
    }

    #[test]
    fn a_palette_change_repaints_the_paused_frame() {
        let mut engine = Engine::new(&test_rom()).unwrap();
        engine.step(Buttons(0), Buttons(0));
        let before = engine.frame()[0..3].to_vec();
        engine.set_palette([0x123456; 64]);
        assert_eq!(&engine.frame()[0..3], &[0x12, 0x34, 0x56]);
        assert_ne!(before, engine.frame()[0..3]);
    }

    #[test]
    fn save_state_restores_the_machine_and_replays() {
        let mut engine = Engine::new(&test_rom()).unwrap();
        engine.step(Buttons(0), Buttons(0));
        let saved = engine.save_state();
        engine.step(Buttons(0), Buttons(0));
        let expected = engine.save_state();
        engine.load_state(&saved).unwrap();
        assert_eq!(engine.rewind_depth(), 0);
        engine.step(Buttons(0), Buttons(0));
        assert_eq!(engine.save_state(), expected);
        assert!(engine.load_state(b"garbage").is_err());
    }

    #[test]
    fn reset_keeps_battery_ram_and_clears_the_chain() {
        use nes_core::cpu::Bus;
        let mut engine = Engine::new(&test_rom()).unwrap();
        engine.nes.bus.write(0x6000, 0x5a);
        engine.step(Buttons(0), Buttons(0));
        engine.reset().unwrap();
        assert_eq!(engine.rewind_depth(), 0);
        assert_eq!(engine.battery_ram().unwrap()[0], 0x5a);
    }

    #[test]
    fn frames_for_uses_the_region_rate() {
        let engine = Engine::new(&test_rom()).unwrap();
        assert_eq!(engine.frames_for(5), (5.0 * engine.frame_rate()) as usize);
        assert!(engine.frames_for(15) > 700);
    }
}
