//! Generated diagnostic output only. No ROM parser, SNES hardware, or shipping loader.
use emulation_api::*;

pub struct SyntheticCore {
    frame: u64,
    buttons: u16,
    pixels: Vec<u8>,
    samples: Vec<f32>,
    descriptor: VideoDescriptor,
}
impl Default for SyntheticCore {
    fn default() -> Self {
        Self::new()
    }
}
impl SyntheticCore {
    pub fn new() -> Self {
        let mut core = Self {
            frame: 0,
            buttons: 0,
            pixels: Vec::new(),
            samples: Vec::new(),
            descriptor: VideoDescriptor {
                width: 1,
                height: 1,
                stride_bytes: 4,
                format: PixelFormat::Rgba8888,
                visible: Rect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                },
                pixel_aspect: (1, 1),
                presentation: Presentation::Progressive,
            },
        };
        core.paint();
        core
    }
    fn paint(&mut self) {
        // Includes row padding, a nonzero visible origin, high resolution and
        // a shrink after growth. Padding must never become a displayed pixel.
        let (width, height, pad) = match self.frame % 3 {
            0 => (256, 224, 0),
            1 => (512, 478, 16),
            _ => (320, 240, 8),
        };
        self.descriptor = VideoDescriptor {
            width,
            height,
            stride_bytes: width * 4 + pad,
            format: PixelFormat::Rgba8888,
            visible: Rect {
                x: 4,
                y: 2,
                width: width - 8,
                height: height - 4,
            },
            pixel_aspect: (8, 7),
            presentation: if height == 478 {
                Presentation::Interlaced
            } else {
                Presentation::Progressive
            },
        };
        self.pixels.resize(self.descriptor.stride_bytes * height, 0);
        self.pixels.fill(0xdd);
        for y in 0..height {
            for x in 0..width {
                let at = y * self.descriptor.stride_bytes + x * 4;
                self.pixels[at..at + 4].copy_from_slice(&[
                    x as u8,
                    y as u8,
                    self.buttons as u8,
                    255,
                ]);
            }
        }
    }
}
impl EmbeddedCore for SyntheticCore {
    fn identity(&self) -> CoreIdentity {
        CoreIdentity {
            system: SystemId::Diagnostic,
            id: "synthetic",
            version: "1",
            state_format: "none",
            state_version: 0,
        }
    }
    fn content_identity(&self) -> ContentIdentity {
        ContentIdentity {
            scheme: "diagnostic",
            value: "generated-media-v1".into(),
        }
    }
    fn capabilities(&self) -> CoreCapabilities {
        CoreCapabilities {
            save_states: false,
            battery_save: false,
            frame_stepping: true,
            input_ports: 1,
            supported_buttons: Buttons::A | Buttons::L | Buttons::R,
        }
    }
    fn timing(&self) -> Timing {
        Timing {
            frames_per_second: 60.0,
        }
    }
    fn set_input(&mut self, port: usize, buttons: Buttons) -> Result<(), CoreError> {
        if port != 0 || buttons.0 & !self.capabilities().supported_buttons != 0 {
            return Err(CoreError::Unsupported("diagnostic input"));
        }
        self.buttons = buttons.0;
        Ok(())
    }
    fn step_frame(&mut self) -> Result<(), CoreError> {
        self.frame += 1;
        self.paint();
        self.samples.clear();
        for _ in 0..800 {
            self.samples.extend_from_slice(&[0.25, -0.5]);
        }
        Ok(())
    }
    fn video(&self) -> VideoFrame<'_> {
        VideoFrame {
            descriptor: self.descriptor,
            pixels: &self.pixels,
        }
    }
    fn audio(&self) -> AudioFrame<'_> {
        AudioFrame {
            format: AudioFormat {
                sample_rate: 48_000,
                channels: 2,
            },
            samples: &self.samples,
        }
    }
    fn reset(&mut self) -> Result<(), CoreError> {
        *self = Self::new();
        Ok(())
    }
    fn snapshot(&self) -> Result<Vec<u8>, CoreError> {
        Err(CoreError::Unsupported("save states"))
    }
    fn load_state(&mut self, _: &[u8]) -> Result<(), CoreError> {
        Err(CoreError::Unsupported("save states"))
    }
    fn battery_ram(&self) -> Option<&[u8]> {
        None
    }
    fn load_battery_ram(&mut self, _: &[u8]) -> Result<(), CoreError> {
        Err(CoreError::Unsupported("battery saves"))
    }
}
