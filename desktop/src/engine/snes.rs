//! SNES machine and opaque rewind history; platform I/O remains in Session.
use emulation_api::{Buttons, EmbeddedCore, VideoDescriptor};
use emulation_runtime::history::Rewind;
use snes_adapter::SnesAdapter;

pub struct Engine {
    core: SnesAdapter,
    rewind: Rewind,
    rom: Vec<u8>,
}

impl Engine {
    pub fn new(rom: &[u8]) -> Result<Self, String> {
        let core = SnesAdapter::new(rom).map_err(|e| e.to_string())?;
        let mut engine = Self {
            core,
            rewind: Rewind::new(super::nes::REWIND_BUDGET),
            rom: rom.to_vec(),
        };
        engine.anchor()?;
        Ok(engine)
    }
    fn anchor(&mut self) -> Result<(), String> {
        let state = self.save_state()?;
        self.rewind.clear();
        self.rewind.push(&state);
        Ok(())
    }
    pub fn id(&self) -> String {
        self.core.content_identity().value
    }
    pub fn frame_rate(&self) -> f64 {
        self.core.timing().frames_per_second
    }
    pub fn video_descriptor(&self) -> VideoDescriptor {
        self.core.video().descriptor
    }
    pub fn frame(&self) -> &[u8] {
        self.core.video().pixels
    }
    pub fn samples(&self) -> &[f32] {
        self.core.audio().samples
    }
    pub fn step(&mut self, p1: Buttons, p2: Buttons) -> Result<(), String> {
        self.core.set_input(0, p1).map_err(|e| e.to_string())?;
        self.core.set_input(1, p2).map_err(|e| e.to_string())?;
        self.core.step_frame().map_err(|e| e.to_string())?;
        self.rewind
            .push(&self.core.snapshot().map_err(|e| e.to_string())?);
        Ok(())
    }
    pub fn rewind_step(&mut self) -> Result<bool, String> {
        self.rewind
            .restore(|state| self.core.load_state(state))
            .map_err(|e| e.to_string())
    }
    pub fn rewind_depth(&self) -> usize {
        self.rewind.depth()
    }
    pub fn save_state(&self) -> Result<Vec<u8>, String> {
        self.core.snapshot().map_err(|e| e.to_string())
    }
    pub fn load_state(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.core.load_state(bytes).map_err(|e| e.to_string())?;
        self.anchor()
    }
    pub fn battery_ram(&self) -> Option<&[u8]> {
        self.core.battery_ram()
    }
    pub fn load_battery(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.core
            .load_battery_ram(bytes)
            .map_err(|e| e.to_string())?;
        self.anchor()
    }
    pub fn reset(&mut self) -> Result<(), String> {
        let mut fresh = Self::new(&self.rom)?;
        if let Some(battery) = self.battery_ram() {
            fresh.load_battery(battery)?;
        }
        *self = fresh;
        Ok(())
    }
}
