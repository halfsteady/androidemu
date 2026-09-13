//! In-process core boundary. No platform I/O, scheduling, or external launchers.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemId {
    Nes,
    Snes,
    Diagnostic,
}

impl SystemId {
    /// Explicit storage identifiers; enum ordinals are never a file format.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Nes => "nes",
            Self::Snes => "snes",
            Self::Diagnostic => "diagnostic",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreIdentity {
    pub system: SystemId,
    pub id: &'static str,
    /// Implementation version, distinct from the opaque state format version.
    pub version: &'static str,
    pub state_format: &'static str,
    pub state_version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentIdentity {
    pub scheme: &'static str,
    pub value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreCapabilities {
    pub save_states: bool,
    pub battery_save: bool,
    /// Host controls execution; this is not a performance or rewind qualification.
    pub frame_stepping: bool,
    pub input_ports: u8,
    pub supported_buttons: u16,
}

/// Stable logical bits. Shoulders are gameplay inputs, never time shortcuts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Buttons(pub u16);
impl Buttons {
    pub const A: u16 = 1 << 0;
    pub const B: u16 = 1 << 1;
    pub const SELECT: u16 = 1 << 2;
    pub const START: u16 = 1 << 3;
    pub const UP: u16 = 1 << 4;
    pub const DOWN: u16 = 1 << 5;
    pub const LEFT: u16 = 1 << 6;
    pub const RIGHT: u16 = 1 << 7;
    pub const X: u16 = 1 << 8;
    pub const Y: u16 = 1 << 9;
    pub const L: u16 = 1 << 10;
    pub const R: u16 = 1 << 11;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Rgba8888,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presentation {
    Progressive,
    Interlaced,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoDescriptor {
    pub width: usize,
    pub height: usize,
    pub stride_bytes: usize,
    pub format: PixelFormat,
    pub visible: Rect,
    /// Width/height of one pixel, not the entire display. Frontends may override.
    pub pixel_aspect: (u32, u32),
    pub presentation: Presentation,
}
impl VideoDescriptor {
    /// Validate all geometry before a host uses a core-provided buffer.
    pub fn required_bytes(self) -> Result<usize, CoreError> {
        let invalid = || CoreError::Invalid("Invalid video descriptor".into());
        let row = self.width.checked_mul(4).ok_or_else(invalid)?;
        if self.width == 0
            || self.height == 0
            || self.stride_bytes < row
            || self.visible.width == 0
            || self.visible.height == 0
            || self
                .visible
                .x
                .checked_add(self.visible.width)
                .ok_or_else(invalid)?
                > self.width
            || self
                .visible
                .y
                .checked_add(self.visible.height)
                .ok_or_else(invalid)?
                > self.height
            || self.pixel_aspect.0 == 0
            || self.pixel_aspect.1 == 0
        {
            return Err(invalid());
        }
        self.stride_bytes
            .checked_mul(self.height)
            .ok_or_else(invalid)
    }
}
pub struct VideoFrame<'a> {
    pub descriptor: VideoDescriptor,
    pub pixels: &'a [u8],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u8,
}
pub struct AudioFrame<'a> {
    pub format: AudioFormat,
    /// Interleaved f32 samples. Audio frames = samples.len() / channels.
    pub samples: &'a [f32],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timing {
    pub frames_per_second: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    Invalid(String),
    Unsupported(&'static str),
}
impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::Unsupported(operation) => write!(f, "Unsupported operation: {operation}"),
        }
    }
}
impl std::error::Error for CoreError {}

/// One serialized owner calls all methods. No wall-clock throttling or audio I/O.
/// Borrowed video/audio/battery buffers belong to the core and remain valid until
/// the next mutable call. Copy before queuing to another thread. Audio is only the
/// latest completed frame, consumed once by the host; stepping replaces it.
/// Failed input/persistence operations must leave the session intact. Hosts flush
/// queued audio/video after restore/reset. Resampling belongs to the host.
pub trait EmbeddedCore {
    fn identity(&self) -> CoreIdentity;
    fn content_identity(&self) -> ContentIdentity;
    fn capabilities(&self) -> CoreCapabilities;
    fn timing(&self) -> Timing;
    fn set_input(&mut self, port: usize, buttons: Buttons) -> Result<(), CoreError>;
    fn step_frame(&mut self) -> Result<(), CoreError>;
    fn video(&self) -> VideoFrame<'_>;
    fn audio(&self) -> AudioFrame<'_>;
    /// Hardware reset. Power cycling with ROM validation is adapter-specific.
    fn reset(&mut self) -> Result<(), CoreError>;
    fn snapshot(&self) -> Result<Vec<u8>, CoreError>;
    fn load_state(&mut self, bytes: &[u8]) -> Result<(), CoreError>;
    fn battery_ram(&self) -> Option<&[u8]>;
    fn load_battery_ram(&mut self, bytes: &[u8]) -> Result<(), CoreError>;
}

/// Static factory; no dynamic loader or stable third-party ABI is implied.
pub trait LoadCore: EmbeddedCore + Sized {
    fn load(rom: &[u8]) -> Result<Self, CoreError>;
}
