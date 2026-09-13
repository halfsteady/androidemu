//! NES-specific translation at frame boundaries. Hardware remains in nes-core.
use emulation_api::*;
use nes_core::Nes;

mod palette;
pub use palette::PALETTE as DEFAULT_PALETTE;

pub struct NesAdapter {
    nes: Nes,
    rgba: Vec<u8>,
    palette: [u32; 64],
    audio_ready: bool,
}

impl NesAdapter {
    pub fn new(rom: &[u8]) -> Result<Self, CoreError> {
        Self::load(rom)
    }
    /// Legacy Android library/save identity. Do not substitute desktop's hash.
    pub fn android_content_id(&self) -> String {
        format!("{:016x}", self.nes.bus.cart.header.hash)
    }
    /// Legacy SDL filenames include the original payload, including trailing data.
    pub fn desktop_content_id(&self) -> String {
        format!("{:016x}", self.nes.bus.cart.header.identity)
    }
    pub fn header_notes(&self) -> Vec<&'static str> {
        self.nes.bus.cart.header.fixes.labels().collect()
    }
    pub fn set_palette(&mut self, palette: [u32; 64]) {
        if self.palette != palette {
            self.palette = palette;
            self.paint();
        }
    }
    /// Existing ANES writer, byte-for-byte; useful to the legacy rewind host.
    pub fn save_state(&self) -> Vec<u8> {
        self.nes.save_state()
    }
    /// Android's fresh power-on, retaining battery RAM and the chosen palette.
    /// Construct and validate first so failure cannot damage the live session.
    pub fn power_cycle(&mut self, rom: &[u8]) -> Result<(), CoreError> {
        let mut fresh = Self::new(rom)?;
        if fresh.nes.bus.cart.header != self.nes.bus.cart.header {
            return Err(CoreError::Invalid(
                "Reset requires the current game's ROM".into(),
            ));
        }
        if let Some(battery) = self.battery_ram() {
            fresh.load_battery_ram(battery)?;
        }
        fresh.set_palette(self.palette);
        *self = fresh;
        Ok(())
    }
    fn paint(&mut self) {
        for (pixel, &index) in self.rgba.chunks_exact_mut(4).zip(self.nes.framebuffer()) {
            let rgb = self.palette[(index & 63) as usize];
            pixel.copy_from_slice(&[(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255]);
        }
    }
}

fn invalid(error: impl ToString) -> CoreError {
    CoreError::Invalid(error.to_string())
}

impl LoadCore for NesAdapter {
    fn load(rom: &[u8]) -> Result<Self, CoreError> {
        let mut adapter = Self {
            nes: Nes::new(rom).map_err(invalid)?,
            rgba: vec![0; 256 * 240 * 4],
            palette: DEFAULT_PALETTE,
            audio_ready: false,
        };
        adapter.paint();
        Ok(adapter)
    }
}

impl EmbeddedCore for NesAdapter {
    fn identity(&self) -> CoreIdentity {
        CoreIdentity {
            system: SystemId::Nes,
            id: "nes-core",
            version: env!("CARGO_PKG_VERSION"),
            state_format: "ANES",
            state_version: 3,
        }
    }
    fn content_identity(&self) -> ContentIdentity {
        ContentIdentity {
            scheme: "nes-payload-fnv1a64",
            value: self.desktop_content_id(),
        }
    }
    fn capabilities(&self) -> CoreCapabilities {
        CoreCapabilities {
            save_states: true,
            battery_save: self.battery_ram().is_some(),
            frame_stepping: true,
            input_ports: 2,
            supported_buttons: 0xff,
        }
    }
    fn timing(&self) -> Timing {
        Timing {
            frames_per_second: self.nes.bus.cart.header.region.frame_rate(),
        }
    }
    fn set_input(&mut self, port: usize, buttons: Buttons) -> Result<(), CoreError> {
        if port >= 2 {
            return Err(CoreError::Unsupported("input port"));
        }
        if buttons.0 & !0xff != 0 {
            return Err(CoreError::Unsupported("input button"));
        }
        self.nes
            .set_buttons(port, nes_core::Buttons(buttons.0 as u8));
        Ok(())
    }
    fn step_frame(&mut self) -> Result<(), CoreError> {
        self.nes.step_frame();
        self.audio_ready = true;
        self.paint();
        Ok(())
    }
    fn video(&self) -> VideoFrame<'_> {
        VideoFrame {
            descriptor: VideoDescriptor {
                width: 256,
                height: 240,
                stride_bytes: 256 * 4,
                format: PixelFormat::Rgba8888,
                visible: Rect {
                    x: 0,
                    y: 0,
                    width: 256,
                    height: 240,
                },
                // Hardware metadata; the legacy SDL host explicitly chooses square pixels.
                pixel_aspect: (8, 7),
                presentation: Presentation::Progressive,
            },
            pixels: &self.rgba,
        }
    }
    fn audio(&self) -> AudioFrame<'_> {
        AudioFrame {
            format: AudioFormat {
                sample_rate: 48_000,
                channels: 1,
            },
            samples: if self.audio_ready {
                self.nes.bus.apu.samples()
            } else {
                &[]
            },
        }
    }
    fn reset(&mut self) -> Result<(), CoreError> {
        self.nes.reset();
        self.audio_ready = false;
        self.paint();
        Ok(())
    }
    fn snapshot(&self) -> Result<Vec<u8>, CoreError> {
        Ok(self.save_state())
    }
    fn load_state(&mut self, bytes: &[u8]) -> Result<(), CoreError> {
        self.nes.load_state(bytes).map_err(invalid)?;
        self.audio_ready = false;
        self.paint();
        Ok(())
    }
    fn battery_ram(&self) -> Option<&[u8]> {
        self.nes.battery_ram()
    }
    fn load_battery_ram(&mut self, bytes: &[u8]) -> Result<(), CoreError> {
        if self.battery_ram().is_none() {
            return Err(CoreError::Unsupported("battery save"));
        }
        self.nes.load_battery_ram(bytes).map_err(invalid)
    }
}
