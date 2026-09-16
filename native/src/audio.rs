//! AAudio callback with a bounded single-producer/single-consumer queue.
//!
//! # Where the latency actually is
//!
//! We control two buffers: the source queue and AAudio's output buffer. With
//! low-latency output, two AAudio bursts can be just a few milliseconds; the
//! source queue is then the larger part, forced by when samples arrive. Other
//! mixers, drivers and the TV can add delay beyond these buffer estimates.
//!
//! `Native_frame` pushes a whole frame of audio at once, because it is called
//! once per displayed frame. So ~800 samples (at 48 kHz, NTSC) land in a single
//! instant every 16.7 ms and then nothing arrives at all. The queue drains
//! smoothly between those deliveries, which means its level sawtooths across a
//! full frame's worth of samples. For the trough of that sawtooth to stay above
//! empty, the *mean* level cannot go below about half a frame — roughly 8 ms —
//! plus whatever margin the display's own jitter needs.
//!
//! That is the real floor, and it is why the plan's sub-10 ms end-to-end target
//! is **not reached**: queue plus output buffer is already around 12-14 ms on
//! suitable low-latency hardware, before unmeasured downstream delay.
//! Going below it needs audio production decoupled from the display clock, which
//! is an architectural change and not a constant to tune. See docs/AUDIO.md.
//!
//! What this module does do is find the lowest target the device will actually
//! tolerate, instead of assuming one: it starts a little under a frame, rises
//! when it hears an underrun, and decays slowly back toward the floor. It also
//! reports what it settled on, so the number is measurable rather than a guess.
//!
//! Those small buffers assume the requested low-latency mode was granted. A
//! normal mixer can consume much larger batches. On that fallback path we keep
//! AAudio's default buffer and use it as a conservative batching window for the
//! source queue. This prevents gaps; it does not remove the platform's latency.
use std::{
    ffi::c_void,
    sync::{
        atomic::{AtomicU32, AtomicUsize, Ordering::*},
        Mutex,
    },
};
// Leave room for the fallback controller's two-window ceiling, plus a frame.
const CAPACITY: usize = 32768;
const LOW_LATENCY: i32 = 12;
/// The rate the core resamples to; the callback converts from here to the device.
const CORE_RATE: f64 = 48000.0;
static SAMPLES: [AtomicU32; CAPACITY] = [const { AtomicU32::new(0) }; CAPACITY];
static READ: AtomicUsize = AtomicUsize::new(0);
static WRITE: AtomicUsize = AtomicUsize::new(0);
static RATE: AtomicU32 = AtomicU32::new(48000);
static STREAM: Mutex<usize> = Mutex::new(0);

/// Core samples delivered per emulated frame: the low-latency queue's window.
static PER_FRAME: AtomicU32 = AtomicU32::new(800);
/// Samples consumed as a batch: one NES frame on a low-latency stream, or a
/// conservative window from AAudio's granted buffer on a normal mixer.
static WINDOW: AtomicU32 = AtomicU32::new(800);
/// Where the queue is being held, in core samples. Adapts; see the module note.
static TARGET: AtomicU32 = AtomicU32::new(800);
/// Callbacks that ran the source queue dry, separate from platform underruns.
static UNDERRUNS: AtomicU32 = AtomicU32::new(0);
static OUTPUT_UNDERRUNS: AtomicU32 = AtomicU32::new(0);
/// Smoothed queue level in core samples, as f32 bits. Written only by the callback.
static LEVEL: AtomicU32 = AtomicU32::new(0);
/// What AAudio gave us, for the reported latency figure.
static BUFFER_FRAMES: AtomicU32 = AtomicU32::new(0);
/// Callbacks since the last underrun, so the target only creeps down when clean.
static CLEAN: AtomicU32 = AtomicU32::new(0);

#[link(name = "aaudio")]
extern "C" {
    fn AAudio_createStreamBuilder(builder: *mut *mut c_void) -> i32;
    fn AAudioStreamBuilder_delete(builder: *mut c_void) -> i32;
    fn AAudioStreamBuilder_setDirection(builder: *mut c_void, direction: i32);
    fn AAudioStreamBuilder_setFormat(builder: *mut c_void, format: i32);
    fn AAudioStreamBuilder_setChannelCount(builder: *mut c_void, channels: i32);
    fn AAudioStreamBuilder_setPerformanceMode(builder: *mut c_void, mode: i32);
    fn AAudioStreamBuilder_setSharingMode(builder: *mut c_void, mode: i32);
    fn AAudioStreamBuilder_setUsage(builder: *mut c_void, usage: i32);
    fn AAudioStreamBuilder_setDataCallback(
        builder: *mut c_void,
        callback: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, i32) -> i32,
        data: *mut c_void,
    );
    fn AAudioStreamBuilder_openStream(builder: *mut c_void, stream: *mut *mut c_void) -> i32;
    fn AAudioStream_getSampleRate(stream: *mut c_void) -> i32;
    fn AAudioStream_getFramesPerBurst(stream: *mut c_void) -> i32;
    fn AAudioStream_getPerformanceMode(stream: *mut c_void) -> i32;
    fn AAudioStream_getXRunCount(stream: *mut c_void) -> i32;
    fn AAudioStream_getBufferSizeInFrames(stream: *mut c_void) -> i32;
    fn AAudioStream_setBufferSizeInFrames(stream: *mut c_void, frames: i32) -> i32;
    fn AAudioStream_requestStart(stream: *mut c_void) -> i32;
    fn AAudioStream_requestStop(stream: *mut c_void) -> i32;
    fn AAudioStream_close(stream: *mut c_void) -> i32;
}

// The audio callback alone accesses this state, and close joins that callback.
struct Resampler {
    primed: bool,
    phase: f64,
    previous: f32,
    next: f32,
}
static mut RESAMPLER: Resampler = Resampler { primed: false, phase: 0.0, previous: 0.0, next: 0.0 };

/// How many core samples one emulated frame delivers, from the region's rate.
/// Called before the stream starts, so the target is right from the first burst.
pub fn set_frame_rate(frame_rate: f32) {
    let per_frame = (CORE_RATE / frame_rate.max(1.0) as f64).round().max(1.0) as u32;
    PER_FRAME.store(per_frame, Relaxed);
    // Start a little under a whole frame. Low enough to be a real improvement on
    // the old fixed 1000 samples, high enough that a device with ordinary
    // scheduling jitter never has to hear the target being found.
    TARGET.store(per_frame * 3 / 4, Relaxed);
}

pub fn push(samples: &[f32]) {
    let mut write = WRITE.load(Relaxed);
    let read = READ.load(Acquire);
    for &sample in samples {
        if write.wrapping_sub(read) >= CAPACITY {
            break;
        }
        SAMPLES[write % CAPACITY].store(sample.to_bits(), Relaxed);
        write = write.wrapping_add(1);
    }
    WRITE.store(write, Release);
}

/// Source queue and AAudio buffer estimates in ms, plus source underruns.
pub fn stats() -> (f32, f32, u32) {
    let level = f32::from_bits(LEVEL.load(Relaxed));
    let buffer = BUFFER_FRAMES.load(Relaxed) as f32;
    let rate = RATE.load(Relaxed).max(1) as f32;
    (level / CORE_RATE as f32 * 1000.0, buffer / rate * 1000.0, UNDERRUNS.load(Relaxed))
}

/// The target the controller settled on, in milliseconds, for the same reason.
pub fn target_ms() -> f32 {
    TARGET.load(Relaxed) as f32 / CORE_RATE as f32 * 1000.0
}

/// Platform underruns are separate from starvation of our source queue. The
/// latter can stay at zero while an undersized AudioTrack repeatedly runs dry.
/// Read from the control thread only; the audio callback never takes this lock.
pub fn output_underruns() -> u32 {
    let handle = STREAM.lock().unwrap();
    if *handle != 0 {
        let count = unsafe { AAudioStream_getXRunCount(*handle as *mut c_void).max(0) as u32 };
        OUTPUT_UNDERRUNS.store(count, Relaxed);
    }
    OUTPUT_UNDERRUNS.load(Relaxed)
}

unsafe extern "C" fn callback(
    _: *mut c_void,
    _: *mut c_void,
    buffer: *mut c_void,
    frames: i32,
) -> i32 {
    let out = std::slice::from_raw_parts_mut(buffer as *mut f32, frames as usize);
    let fill = WRITE.load(Acquire).wrapping_sub(READ.load(Relaxed));
    let window = WINDOW.load(Relaxed).max(1);
    let mut target = TARGET.load(Relaxed);

    // Smoothed for reporting only; the controller uses the instantaneous level.
    let level = f32::from_bits(LEVEL.load(Relaxed));
    LEVEL.store((level * 0.99 + fill as f32 * 0.01).to_bits(), Relaxed);

    // The stream starts before the first frame is emulated. Let the source
    // queue build its reserve once, instead of repeatedly draining it during
    // startup. This is silence before playback begins, not lost game samples.
    if !RESAMPLER.primed {
        if fill < target as usize {
            out.fill(0.0);
            return 0;
        }
        RESAMPLER.primed = true;
    }

    // Dynamic rate control: nudge the resample ratio to hold the queue at the
    // target. The gain is deliberately small — a tenth of a percent of pitch is
    // inaudible, and anything fast enough to correct in under a second is not.
    let error = fill as f64 - target as f64;
    let correction = (1.0 + error * 0.000005).clamp(0.997, 1.003);
    let step = CORE_RATE / RATE.load(Relaxed) as f64 * correction;

    let mut starved = false;
    for value in out {
        *value = RESAMPLER.previous + (RESAMPLER.next - RESAMPLER.previous) * RESAMPLER.phase as f32;
        RESAMPLER.phase += step;
        while RESAMPLER.phase >= 1.0 {
            RESAMPLER.phase -= 1.0;
            RESAMPLER.previous = RESAMPLER.next;
            let read = READ.load(Relaxed);
            if read == WRITE.load(Acquire) {
                // Nothing left. Hold the last sample rather than slamming to
                // zero: a click is far more audible than a held level.
                starved = true;
            } else {
                RESAMPLER.next = f32::from_bits(SAMPLES[read % CAPACITY].load(Relaxed));
                READ.store(read.wrapping_add(1), Release);
            }
        }
    }

    // One underrun per callback that ran dry, however many samples it wanted.
    if starved {
        UNDERRUNS.fetch_add(1, Relaxed);
        CLEAN.store(0, Relaxed);
        // Back off by a quarter window, up to two windows. Using NES frames as
        // the ceiling on a normal mixer cannot cover its larger drain batches.
        target = (target + window / 4).min(window * 2);
        TARGET.store(target, Relaxed);
    } else {
        // Creep back down, but only after a long clean run, and never below half
        // a window — which is where the sawtooth's trough reaches empty. Probing
        // below that would trade a millisecond for a click every few seconds.
        let clean = CLEAN.fetch_add(1, Relaxed) + 1;
        if clean >= 2000 {
            CLEAN.store(0, Relaxed);
            let floor = (window / 2).max(64);
            if target > floor {
                TARGET.store((target - window / 32).max(floor), Relaxed);
            }
        }
    }
    0
}

pub fn set_playing(playing: bool) -> Result<(), &'static str> {
    let mut handle = STREAM.lock().unwrap();
    unsafe {
        if *handle != 0 {
            if playing {
                return Ok(());
            }
            let stream = *handle as *mut c_void;
            OUTPUT_UNDERRUNS.store(AAudioStream_getXRunCount(stream).max(0) as u32, Relaxed);
            AAudioStream_requestStop(stream);
            AAudioStream_close(stream);
            *handle = 0;
        }
        if !playing {
            READ.store(0, Relaxed);
            WRITE.store(0, Relaxed);
            LEVEL.store(0, Relaxed);
            RESAMPLER = Resampler { primed: false, phase: 0.0, previous: 0.0, next: 0.0 };
            return Ok(());
        }
        let mut builder = std::ptr::null_mut();
        if AAudio_createStreamBuilder(&mut builder) != 0 {
            return Err("Could not create audio output");
        }
        AAudioStreamBuilder_setDirection(builder, 0);
        AAudioStreamBuilder_setFormat(builder, 2);
        AAudioStreamBuilder_setChannelCount(builder, 1);
        AAudioStreamBuilder_setPerformanceMode(builder, LOW_LATENCY);
        AAudioStreamBuilder_setSharingMode(builder, 0);
        AAudioStreamBuilder_setUsage(builder, 14); // AAUDIO_USAGE_GAME
        AAudioStreamBuilder_setDataCallback(builder, callback, std::ptr::null_mut());
        let mut stream = std::ptr::null_mut();
        let mut result = AAudioStreamBuilder_openStream(builder, &mut stream);
        if result != 0 {
            AAudioStreamBuilder_setSharingMode(builder, 1);
            result = AAudioStreamBuilder_openStream(builder, &mut stream);
        }
        AAudioStreamBuilder_delete(builder);
        if result != 0 {
            return Err("Could not open audio output");
        }
        RATE.store(AAudioStream_getSampleRate(stream) as u32, Relaxed);
        // Only shrink a stream that actually obtained low-latency mode. AAudio
        // can open successfully but fall back to a normal mixer. There its
        // callback burst can be smaller than the mixer's consumption quantum:
        // Pi 5 LineageOS grants 960-frame bursts but mixes 4096 frames at once.
        // A 1920-frame buffer then starves on every mixer cycle, even when the
        // callback does nothing except fill silence. Keep the platform's safe
        // default on that path; recompiling for the Pi cannot remove HAL delay.
        let low_latency = AAudioStream_getPerformanceMode(stream) == LOW_LATENCY;
        if low_latency {
            AAudioStream_setBufferSizeInFrames(stream, AAudioStream_getFramesPerBurst(stream) * 2);
        }
        // Read back what was actually granted rather than what was asked for.
        BUFFER_FRAMES.store(AAudioStream_getBufferSizeInFrames(stream).max(0) as u32, Relaxed);
        let window = if low_latency {
            PER_FRAME.load(Relaxed)
        } else {
            // AAudio exposes callback size, not the normal mixer's actual
            // consumption quantum. Its default buffer is a conservative bound.
            (BUFFER_FRAMES.load(Relaxed) as f64 * CORE_RATE / RATE.load(Relaxed).max(1) as f64)
                .ceil() as u32
        }.max(PER_FRAME.load(Relaxed)).min(CAPACITY as u32 / 4);
        WINDOW.store(window, Relaxed);
        TARGET.store(window * 3 / 4, Relaxed);
        UNDERRUNS.store(0, Relaxed);
        OUTPUT_UNDERRUNS.store(0, Relaxed);
        CLEAN.store(0, Relaxed);
        if AAudioStream_requestStart(stream) != 0 {
            AAudioStream_close(stream);
            return Err("Could not start audio output");
        }
        *handle = stream as usize;
    }
    Ok(())
}
