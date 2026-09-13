//! Display-driven scheduling shared by embedded hosts; no wall-clock sleeps.
use crate::history::Rewind;
use emulation_api::{AudioFrame, Buttons, CoreError, EmbeddedCore};

#[derive(Default)]
pub struct FrameClock {
    next: Option<u64>,
}
impl FrameClock {
    pub const fn new() -> Self {
        Self { next: None }
    }
    pub fn due(&mut self, now: u64, fps: f64, active: bool) -> bool {
        if !active || !fps.is_finite() || fps <= 0.0 {
            self.next = None;
            return false;
        }
        let next = self.next.unwrap_or(now);
        let next = if now.saturating_sub(next) > 100_000_000 {
            now
        } else {
            next
        };
        if now < next {
            return false;
        }
        self.next = Some(next.saturating_add((1_000_000_000.0 / fps).max(1.0) as u64));
        true
    }
}

/// Negative requests rewind, zero holds, positive requests execute. Hosts choose
/// their gesture limits. All frames in a batch retain the same logical input.
/// Fast-forward and rewind never deliver audio. Exhaustion never moves forward.
pub fn run_batch(
    core: &mut dyn EmbeddedCore,
    mut history: Option<&mut Rewind>,
    frames: i32,
    inputs: [Buttons; 2],
    mut audio: impl FnMut(AudioFrame<'_>),
) -> Result<usize, CoreError> {
    if frames.unsigned_abs() > 3600 {
        return Err(CoreError::Invalid("Frame batch is too large".into()));
    }
    let caps = core.capabilities();
    if frames < 0 {
        let Some(history) = history else { return Ok(0) };
        let mut count = 0;
        for _ in 0..frames.unsigned_abs() {
            if !history.restore(|state| core.load_state(state))? {
                break;
            }
            count += 1;
        }
        // Input is cleared even when history is exhausted.
        for port in 0..caps.input_ports.min(2) {
            core.set_input(usize::from(port), Buttons(0))?;
        }
        return Ok(count);
    }
    if frames == 0 {
        return Ok(0);
    }
    if caps.input_ports == 0
        || inputs.iter().any(|b| b.0 & !caps.supported_buttons != 0)
        || (caps.input_ports < 2 && inputs[1].0 != 0)
    {
        return Err(CoreError::Invalid("Unsupported input".into()));
    }
    for _ in 0..frames {
        for port in 0..caps.input_ports.min(2) {
            core.set_input(usize::from(port), inputs[usize::from(port)])?;
        }
        core.step_frame()?;
        if let Some(history) = history.as_deref_mut() {
            match core.snapshot() {
                Ok(state) => history.push(&state),
                Err(error) => {
                    history.clear();
                    return Err(error);
                }
            }
        }
        if frames == 1 {
            audio(core.audio());
        }
    }
    Ok(frames as usize)
}
