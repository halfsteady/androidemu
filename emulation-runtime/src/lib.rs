//! Shared host media, history and scheduling. The host retains session ownership.
pub mod history;
pub mod scheduling;
use emulation_api::{AudioFormat, AudioFrame, CoreError, VideoFrame};

/// A host limit, not a claim about a system's hardware modes. Bound allocations
/// before accepting a descriptor; enough for the planned SNES interlaced modes.
pub const MAX_VIDEO_BYTES: usize = 16 * 1024 * 1024;

pub fn validate_video(frame: &VideoFrame<'_>) -> Result<usize, CoreError> {
    let bytes = frame.descriptor.required_bytes()?;
    if bytes > MAX_VIDEO_BYTES || frame.pixels.len() < bytes {
        return Err(CoreError::Invalid("Invalid video buffer length".into()));
    }
    Ok(bytes)
}

/// Copy RGBA into a host-owned texture/buffer, excluding source row padding.
/// Both layouts are checked before any destination byte is changed.
pub fn copy_rgba(frame: &VideoFrame<'_>, out: &mut [u8], stride: usize) -> Result<(), CoreError> {
    validate_video(frame)?;
    let row = frame.descriptor.width * 4;
    let required = stride
        .checked_mul(frame.descriptor.height)
        .ok_or_else(|| CoreError::Invalid("Video destination size overflow".into()))?;
    if stride < row || out.len() < required {
        return Err(CoreError::Invalid("Video destination is too small".into()));
    }
    for y in 0..frame.descriptor.height {
        let src = y * frame.descriptor.stride_bytes;
        out[y * stride..y * stride + row].copy_from_slice(&frame.pixels[src..src + row]);
    }
    Ok(())
}

pub fn validate_audio_format(format: AudioFormat) -> Result<(), CoreError> {
    if !(8_000..=192_000).contains(&format.sample_rate) || !matches!(format.channels, 1 | 2) {
        return Err(CoreError::Unsupported(
            "audio format (requires mono/stereo, 8–192 kHz)",
        ));
    }
    Ok(())
}

pub fn audio_frames(audio: &AudioFrame<'_>) -> Result<usize, CoreError> {
    validate_audio_format(audio.format)?;
    if !audio
        .samples
        .len()
        .is_multiple_of(usize::from(audio.format.channels))
    {
        return Err(CoreError::Invalid(
            "Incomplete interleaved audio frame".into(),
        ));
    }
    Ok(audio.samples.len() / usize::from(audio.format.channels))
}

/// SDL counts queued bytes; latency and resampling count complete audio frames.
pub fn audio_queue_bytes(
    format: AudioFormat,
    fps: f64,
    video_frames: f64,
) -> Result<u32, CoreError> {
    validate_audio_format(format)?;
    if !fps.is_finite() || fps <= 0.0 || !video_frames.is_finite() || video_frames <= 0.0 {
        return Err(CoreError::Invalid("Invalid audio queue timing".into()));
    }
    let bytes =
        (format.sample_rate as f64 / fps * video_frames).ceil() * f64::from(format.channels) * 4.0;
    if bytes > u32::MAX as f64 {
        return Err(CoreError::Invalid("Audio queue size overflow".into()));
    }
    Ok(bytes as u32)
}

/// One interpolation phase shared by both channels. Hosts supply whole frames
/// from their queue, so an underrun or overflow can never swap left and right.
#[derive(Default)]
pub struct Resampler {
    phase: f64,
    previous: [f32; 2],
    next: [f32; 2],
}
impl Resampler {
    pub const fn new() -> Self {
        Self {
            phase: 0.0,
            previous: [0.0; 2],
            next: [0.0; 2],
        }
    }
    pub fn render(
        &mut self,
        out: &mut [f32],
        channels: usize,
        step: f64,
        mut pop: impl FnMut() -> Option<[f32; 2]>,
    ) -> bool {
        assert!(matches!(channels, 1 | 2) && out.len().is_multiple_of(channels));
        assert!(step.is_finite() && step > 0.0);
        let mut starved = false;
        for frame in out.chunks_exact_mut(channels) {
            for (channel, value) in frame.iter_mut().enumerate() {
                *value = self.previous[channel]
                    + (self.next[channel] - self.previous[channel]) * self.phase as f32;
            }
            self.phase += step;
            while self.phase >= 1.0 {
                self.phase -= 1.0;
                self.previous = self.next;
                if let Some(next) = pop() {
                    self.next = next;
                } else {
                    starved = true;
                }
            }
        }
        starved
    }
}
