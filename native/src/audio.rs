//! AAudio callback with a bounded single-producer/single-consumer queue.
//!
//! # Where the latency actually is
//!
//! Two buffers sit between the APU and the speaker: this queue, and AAudio's own
//! ring. The device ring is sized from the burst and is small — a few
//! milliseconds. The queue is the big one, and its size is forced by *when*
//! samples arrive rather than by any choice made here.
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
//! is **not reached**: queue plus device ring lands around 12-14 ms at best.
//! Going below it needs audio production decoupled from the display clock, which
//! is an architectural change and not a constant to tune. See docs/AUDIO.md.
//!
//! What this module does do is find the lowest target the device will actually
//! tolerate, instead of assuming one: it starts a little under a frame, rises
//! when it hears an underrun, and decays slowly back toward the floor. It also
//! reports what it settled on, so the number is measurable rather than a guess.
use std::{
    ffi::c_void,
    sync::{
        atomic::{AtomicU32, AtomicUsize, Ordering::*},
        Mutex,
    },
};
const CAPACITY: usize = 8192;
/// The rate the core resamples to; the callback converts from here to the device.
const CORE_RATE: f64 = 48000.0;
static SAMPLES: [AtomicU32; CAPACITY] = [const { AtomicU32::new(0) }; CAPACITY];
static READ: AtomicUsize = AtomicUsize::new(0);
static WRITE: AtomicUsize = AtomicUsize::new(0);
static RATE: AtomicU32 = AtomicU32::new(48000);
static STREAM: Mutex<usize> = Mutex::new(0);

/// Core samples delivered per emulated frame — the height of the sawtooth, and so
/// the unit everything about the queue target is expressed in.
static PER_FRAME: AtomicU32 = AtomicU32::new(800);
/// Where the queue is being held, in core samples. Adapts; see the module note.
static TARGET: AtomicU32 = AtomicU32::new(800);
/// Callbacks that ran the queue dry. The one number that says the target is wrong.
static UNDERRUNS: AtomicU32 = AtomicU32::new(0);
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
    fn AAudioStreamBuilder_setDataCallback(
        builder: *mut c_void,
        callback: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, i32) -> i32,
        data: *mut c_void,
    );
    fn AAudioStreamBuilder_openStream(builder: *mut c_void, stream: *mut *mut c_void) -> i32;
    fn AAudioStream_getSampleRate(stream: *mut c_void) -> i32;
    fn AAudioStream_getFramesPerBurst(stream: *mut c_void) -> i32;
    fn AAudioStream_getBufferSizeInFrames(stream: *mut c_void) -> i32;
    fn AAudioStream_setBufferSizeInFrames(stream: *mut c_void, frames: i32) -> i32;
    fn AAudioStream_requestStart(stream: *mut c_void) -> i32;
    fn AAudioStream_requestStop(stream: *mut c_void) -> i32;
    fn AAudioStream_close(stream: *mut c_void) -> i32;
}

// The audio callback alone accesses this state, and close joins that callback.
struct Resampler {
    phase: f64,
    previous: f32,
    next: f32,
}
static mut RESAMPLER: Resampler = Resampler { phase: 0.0, previous: 0.0, next: 0.0 };

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

/// Queue level in core samples, smoothed, and the device ring in frames.
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

unsafe extern "C" fn callback(
    _: *mut c_void,
    _: *mut c_void,
    buffer: *mut c_void,
    frames: i32,
) -> i32 {
    let out = std::slice::from_raw_parts_mut(buffer as *mut f32, frames as usize);
    let fill = WRITE.load(Acquire).wrapping_sub(READ.load(Relaxed));
    let per_frame = PER_FRAME.load(Relaxed).max(1);
    let mut target = TARGET.load(Relaxed);

    // Smoothed for reporting only; the controller uses the instantaneous level.
    let level = f32::from_bits(LEVEL.load(Relaxed));
    LEVEL.store((level * 0.99 + fill as f32 * 0.01).to_bits(), Relaxed);

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
        // Back off by a quarter frame, up to two frames. Rising on evidence is
        // the whole point: the floor is a property of the device, not a guess.
        target = (target + per_frame / 4).min(per_frame * 2);
        TARGET.store(target, Relaxed);
    } else {
        // Creep back down, but only after a long clean run, and never below half
        // a frame — which is where the sawtooth's trough reaches empty. Probing
        // below that would trade a millisecond for a click every few seconds.
        let clean = CLEAN.fetch_add(1, Relaxed) + 1;
        if clean >= 2000 {
            CLEAN.store(0, Relaxed);
            let floor = (per_frame / 2).max(64);
            if target > floor {
                TARGET.store((target - per_frame / 32).max(floor), Relaxed);
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
            AAudioStream_requestStop(stream);
            AAudioStream_close(stream);
            *handle = 0;
        }
        if !playing {
            READ.store(0, Relaxed);
            WRITE.store(0, Relaxed);
            LEVEL.store(0, Relaxed);
            RESAMPLER = Resampler { phase: 0.0, previous: 0.0, next: 0.0 };
            return Ok(());
        }
        let mut builder = std::ptr::null_mut();
        if AAudio_createStreamBuilder(&mut builder) != 0 {
            return Err("Could not create audio output");
        }
        AAudioStreamBuilder_setDirection(builder, 0);
        AAudioStreamBuilder_setFormat(builder, 2);
        AAudioStreamBuilder_setChannelCount(builder, 1);
        AAudioStreamBuilder_setPerformanceMode(builder, 12);
        AAudioStreamBuilder_setSharingMode(builder, 0);
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
        // Two bursts is the smallest size that gives the callback a whole burst
        // of slack; one burst leaves no room for a late wake-up at all.
        AAudioStream_setBufferSizeInFrames(stream, AAudioStream_getFramesPerBurst(stream) * 2);
        // Read back what was actually granted rather than what was asked for.
        BUFFER_FRAMES.store(AAudioStream_getBufferSizeInFrames(stream).max(0) as u32, Relaxed);
        UNDERRUNS.store(0, Relaxed);
        CLEAN.store(0, Relaxed);
        if AAudioStream_requestStart(stream) != 0 {
            AAudioStream_close(stream);
            return Err("Could not start audio output");
        }
        *handle = stream as usize;
    }
    Ok(())
}
