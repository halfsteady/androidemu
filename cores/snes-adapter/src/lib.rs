//! jgenesis boundary. Standard LoROM/HiROM only until other hardware is qualified.
use emulation_api::*;
use jgenesis_common::frontend::{self as host, EmulatorTrait};
use sha2::{Digest, Sha256};
use snes_core::{
    api::{CoprocessorRoms, SnesEmulator, SnesEmulatorConfig},
    input::{SnesController, SnesInputs},
};
use std::{collections::HashMap, io};

pub const UPSTREAM: &str = "b1419eface3147568b2247d33b6bdb6695adfe06";
const MAGIC: &[u8] = b"ESNS\x01";
const LIMIT: usize = 8 * 1024 * 1024;
fn config() -> impl bincode::config::Config {
    bincode::config::standard()
        .with_little_endian()
        .with_fixed_int_encoding()
        .with_limit::<LIMIT>()
}
fn invalid(error: impl ToString) -> CoreError {
    CoreError::Invalid(error.to_string())
}

#[derive(Default, bincode::Encode, bincode::Decode)]
struct Picture {
    width: u32,
    height: u32,
    aspect_n: u32,
    aspect_d: u32,
    pixels: Vec<u8>,
}
impl host::Renderer for Picture {
    type Err = io::Error;
    fn render_frame(
        &mut self,
        pixels: &[host::Color],
        size: host::FrameSize,
        _: f64,
        options: host::RenderFrameOptions,
    ) -> io::Result<()> {
        if size.width == 0 || size.height == 0 || size.width > 1024 || size.height > 1024 {
            return Err(io::Error::other("Invalid SNES video dimensions"));
        }
        let count = (size.width * size.height) as usize;
        if pixels.len() < count {
            return Err(io::Error::other("Short SNES video buffer"));
        }
        self.width = size.width;
        self.height = size.height;
        // Upstream supplies a floating ratio; store a bounded rational in the ABI.
        let ratio = options.pixel_aspect_ratio.map(f64::from).unwrap_or(1.0);
        if !ratio.is_finite() || ratio <= 0.0 || ratio > 16.0 {
            return Err(io::Error::other("Invalid SNES aspect"));
        }
        self.aspect_n = (ratio * 1_000_000.0).round() as u32;
        self.aspect_d = 1_000_000;
        self.pixels.clear();
        self.pixels.reserve(count * 4);
        for pixel in &pixels[..count] {
            self.pixels
                .extend_from_slice(&[pixel.r, pixel.g, pixel.b, 255]);
        }
        Ok(())
    }
}
#[derive(Default)]
struct Audio(Vec<f32>);
impl host::AudioOutput for Audio {
    type Err = io::Error;
    fn push_sample(&mut self, left: f64, right: f64) -> io::Result<()> {
        if self.0.len() >= 192_000 {
            return Err(io::Error::other("SNES frame audio limit exceeded"));
        }
        self.0.extend_from_slice(&[left as f32, right as f32]);
        Ok(())
    }
}
#[derive(Default)]
struct Saves(HashMap<String, Vec<u8>>);
impl host::SaveWriter for Saves {
    type Err = io::Error;
    fn load_bytes(&mut self, extension: &str) -> io::Result<Vec<u8>> {
        self.0
            .get(extension)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }
    fn persist_bytes(&mut self, extension: &str, bytes: &[u8]) -> io::Result<()> {
        self.0.insert(extension.into(), bytes.into());
        Ok(())
    }
    fn load_serialized<D: bincode::Decode<()>>(&mut self, extension: &str) -> io::Result<D> {
        bincode::decode_from_slice(&self.load_bytes(extension)?, config())
            .map(|v| v.0)
            .map_err(io::Error::other)
    }
    fn persist_serialized<E: bincode::Encode>(
        &mut self,
        extension: &str,
        value: E,
    ) -> io::Result<()> {
        self.persist_bytes(
            extension,
            &bincode::encode_to_vec(value, config()).map_err(io::Error::other)?,
        )
    }
}

/// Validate standard cartridge metadata before allowing upstream autodetection.
fn canonical_rom(bytes: &[u8]) -> Result<&[u8], CoreError> {
    let bytes = if bytes.len() % 32768 == 512 {
        &bytes[512..]
    } else {
        bytes
    };
    if !(32768..=4 * 1024 * 1024).contains(&bytes.len()) {
        return Err(invalid(
            "SNES ROM must be 32 KiB–4 MiB for the initial cartridge set",
        ));
    }
    let mut best = None;
    for (at, mode) in [(0x7fc0, 0x20), (0xffc0, 0x21)] {
        let Some(header) = bytes.get(at..at + 64) else {
            continue;
        };
        if header[0x15] & 0xef != mode {
            continue;
        }
        let reset = u16::from_le_bytes([header[0x3c], header[0x3d]]);
        if reset < 0x8000 {
            continue;
        }
        let checksum = u16::from_le_bytes([header[0x1e], header[0x1f]]);
        let complement = u16::from_le_bytes([header[0x1c], header[0x1d]]);
        let title = header[..21]
            .iter()
            .all(|&b| b == 0 || (0x20..=0x7e).contains(&b));
        let score = usize::from(checksum ^ complement == 0xffff) * 2 + usize::from(title);
        if score == 0 {
            continue;
        }
        if best.is_none_or(|(old, _)| score > old) {
            best = Some((score, at));
        }
    }
    let (_, at) =
        best.ok_or_else(|| invalid("Could not identify a standard SNES cartridge header"))?;
    if bytes[at + 0x16] > 2 {
        return Err(CoreError::Unsupported(
            "SNES enhancement-chip cartridges (not yet qualified)",
        ));
    }
    if bytes[at + 0x18] > 8 {
        return Err(invalid("SNES SRAM size exceeds the supported limit"));
    }
    Ok(bytes)
}

pub struct SnesAdapter {
    core: SnesEmulator,
    rom: Vec<u8>,
    hash: [u8; 32],
    picture: Picture,
    audio: Audio,
    saves: Saves,
    inputs: [Buttons; 2],
}
impl SnesAdapter {
    pub fn new(rom: &[u8]) -> Result<Self, CoreError> {
        Self::load(rom)
    }
    pub fn content_id(rom: &[u8]) -> Result<String, CoreError> {
        let hash = Sha256::digest(canonical_rom(rom)?);
        Ok(format!("snes-{hash:x}"))
    }
    fn input_snapshot(&self) -> SnesInputs {
        let pad = |Buttons(bits): Buttons| {
            SnesController::Gamepad(snes_config::SnesJoypadState {
                a: bits & Buttons::A != 0,
                b: bits & Buttons::B != 0,
                x: bits & Buttons::X != 0,
                y: bits & Buttons::Y != 0,
                l: bits & Buttons::L != 0,
                r: bits & Buttons::R != 0,
                select: bits & Buttons::SELECT != 0,
                start: bits & Buttons::START != 0,
                up: bits & Buttons::UP != 0,
                down: bits & Buttons::DOWN != 0,
                left: bits & Buttons::LEFT != 0,
                right: bits & Buttons::RIGHT != 0,
            })
        };
        SnesInputs {
            p1: pad(self.inputs[0]),
            p2: pad(self.inputs[1]),
        }
    }
}
impl LoadCore for SnesAdapter {
    fn load(bytes: &[u8]) -> Result<Self, CoreError> {
        let rom = canonical_rom(bytes)?.to_vec();
        let hash = Sha256::digest(&rom).into();
        let mut saves = Saves::default();
        let settings = SnesEmulatorConfig {
            deinterlace: false,
            ..Default::default()
        };
        let mut core =
            SnesEmulator::create(rom.clone(), settings, CoprocessorRoms::none(), &mut saves)
                .map_err(invalid)?;
        if !core.is_standard_cartridge() {
            return Err(CoreError::Unsupported(
                "SNES enhancement-chip cartridges (not yet qualified)",
            ));
        }
        core.update_audio_output_frequency(48_000);
        let mut picture = Picture::default();
        core.force_render(&mut picture).map_err(invalid)?;
        Ok(Self {
            core,
            rom,
            hash,
            picture,
            audio: Audio::default(),
            saves,
            inputs: [Buttons(0); 2],
        })
    }
}
impl EmbeddedCore for SnesAdapter {
    fn identity(&self) -> CoreIdentity {
        CoreIdentity {
            system: SystemId::Snes,
            id: "jgenesis-snes",
            version: UPSTREAM,
            state_format: "ESNS",
            state_version: 1,
        }
    }
    fn content_identity(&self) -> ContentIdentity {
        ContentIdentity {
            scheme: "snes-canonical-sha256",
            value: format!("snes-{:x}", Sha256::digest(&self.rom)),
        }
    }
    fn capabilities(&self) -> CoreCapabilities {
        CoreCapabilities {
            save_states: true,
            battery_save: self.battery_ram().is_some(),
            frame_stepping: true,
            input_ports: 2,
            supported_buttons: 0xfff,
        }
    }
    fn timing(&self) -> Timing {
        Timing {
            frames_per_second: self.core.target_fps(),
        }
    }
    fn set_input(&mut self, port: usize, buttons: Buttons) -> Result<(), CoreError> {
        if port >= 2 || buttons.0 & !0xfff != 0 {
            return Err(invalid("Unsupported SNES input"));
        }
        self.inputs[port] = buttons;
        Ok(())
    }
    fn step_frame(&mut self) -> Result<(), CoreError> {
        self.audio.0.clear();
        let inputs = self.input_snapshot();
        for _ in 0..2_000_000 {
            let effect = self
                .core
                .tick(
                    &mut self.picture,
                    &mut self.audio,
                    &mut host::ConstantInputPoller(&inputs),
                    &mut self.saves,
                )
                .map_err(invalid)?;
            if effect == host::TickEffect::FrameRendered {
                return Ok(());
            }
        }
        Err(invalid("SNES frame exceeded its execution budget"))
    }
    fn video(&self) -> VideoFrame<'_> {
        VideoFrame {
            descriptor: VideoDescriptor {
                width: self.picture.width as usize,
                height: self.picture.height as usize,
                stride_bytes: self.picture.width as usize * 4,
                format: PixelFormat::Rgba8888,
                visible: Rect {
                    x: 0,
                    y: 0,
                    width: self.picture.width as usize,
                    height: self.picture.height as usize,
                },
                pixel_aspect: (self.picture.aspect_n, self.picture.aspect_d),
                presentation: if self.picture.height > 300 {
                    Presentation::Interlaced
                } else {
                    Presentation::Progressive
                },
            },
            pixels: &self.picture.pixels,
        }
    }
    fn audio(&self) -> AudioFrame<'_> {
        AudioFrame {
            format: AudioFormat {
                sample_rate: 48_000,
                channels: 2,
            },
            samples: &self.audio.0,
        }
    }
    fn reset(&mut self) -> Result<(), CoreError> {
        self.core.soft_reset();
        self.inputs = [Buttons(0); 2];
        self.audio.0.clear();
        self.core.force_render(&mut self.picture).map_err(invalid)
    }
    fn snapshot(&self) -> Result<Vec<u8>, CoreError> {
        let payload = bincode::encode_to_vec(
            (
                self.core.to_save_state(),
                &self.picture,
                self.inputs.map(|v| v.0),
            ),
            config(),
        )
        .map_err(invalid)?;
        if payload.len() > LIMIT {
            return Err(invalid("SNES snapshot exceeds limit"));
        }
        let mut out = Vec::with_capacity(109 + payload.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(UPSTREAM.as_bytes());
        out.extend_from_slice(&self.hash);
        out.extend_from_slice(&Sha256::digest(&payload));
        out.extend_from_slice(&payload);
        Ok(out)
    }
    fn load_state(&mut self, bytes: &[u8]) -> Result<(), CoreError> {
        if bytes.len() < 109
            || bytes.len() > LIMIT + 109
            || &bytes[..5] != MAGIC
            || &bytes[5..45] != UPSTREAM.as_bytes()
            || bytes[45..77] != self.hash
            || bytes[77..109] != Sha256::digest(&bytes[109..])[..]
        {
            return Err(invalid(
                "SNES state is damaged or belongs to a different game/core version",
            ));
        }
        let ((state, picture, inputs), used): ((SnesEmulator, Picture, [u16; 2]), usize) =
            bincode::decode_from_slice(&bytes[109..], config()).map_err(invalid)?;
        if !state.is_standard_cartridge()
            || used != bytes.len() - 109
            || picture.width == 0
            || picture.height == 0
            || picture.width > 1024
            || picture.height > 1024
            || picture.pixels.len() != picture.width as usize * picture.height as usize * 4
            || picture.aspect_n == 0
            || picture.aspect_d == 0
            || inputs.iter().any(|v| v & !0xfff != 0)
            || state.battery_ram().map(<[u8]>::len) != self.core.battery_ram().map(<[u8]>::len)
        {
            return Err(invalid("Invalid SNES state metadata"));
        }
        self.core.load_state(state);
        self.picture = picture;
        self.inputs = inputs.map(Buttons);
        self.audio.0.clear();
        self.saves.0.clear();
        Ok(())
    }
    fn battery_ram(&self) -> Option<&[u8]> {
        self.core.battery_ram()
    }
    fn load_battery_ram(&mut self, bytes: &[u8]) -> Result<(), CoreError> {
        if !self.core.load_standard_battery_ram(bytes) {
            return Err(invalid("Invalid SNES battery RAM length or cartridge"));
        }
        Ok(())
    }
}
