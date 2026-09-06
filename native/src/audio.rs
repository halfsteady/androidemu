//! AAudio callback with a bounded single-producer/single-consumer queue.
use std::{
    ffi::c_void,
    sync::{
        atomic::{AtomicU32, AtomicUsize, Ordering::*},
        Mutex,
    },
};
const CAPACITY: usize = 8192;
static SAMPLES: [AtomicU32; CAPACITY] = [const { AtomicU32::new(0) }; CAPACITY];
static READ: AtomicUsize = AtomicUsize::new(0);
static WRITE: AtomicUsize = AtomicUsize::new(0);
static RATE: AtomicU32 = AtomicU32::new(48000);
static STREAM: Mutex<usize> = Mutex::new(0);
// The audio callback alone accesses this state, and close joins that callback.
struct Resampler {
    phase: f64,
    previous: f32,
    next: f32,
}
static mut RESAMPLER: Resampler = Resampler {
    phase: 0.0,
    previous: 0.0,
    next: 0.0,
};
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
    fn AAudioStream_setBufferSizeInFrames(stream: *mut c_void, frames: i32) -> i32;
    fn AAudioStream_requestStart(stream: *mut c_void) -> i32;
    fn AAudioStream_requestStop(stream: *mut c_void) -> i32;
    fn AAudioStream_close(stream: *mut c_void) -> i32;
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
fn pop() -> f32 {
    let read = READ.load(Relaxed);
    if read == WRITE.load(Acquire) {
        return 0.0;
    }
    let value = f32::from_bits(SAMPLES[read % CAPACITY].load(Relaxed));
    READ.store(read.wrapping_add(1), Release);
    value
}
unsafe extern "C" fn callback(
    _: *mut c_void,
    _: *mut c_void,
    buffer: *mut c_void,
    frames: i32,
) -> i32 {
    let out = std::slice::from_raw_parts_mut(buffer as *mut f32, frames as usize);
    let fill = WRITE.load(Acquire).wrapping_sub(READ.load(Relaxed)) as f64;
    let correction = (1.0 + (fill - 1000.0) * 0.000005).clamp(0.995, 1.005);
    let step = 48000.0 / RATE.load(Relaxed) as f64 * correction;
    for value in out {
        *value =
            RESAMPLER.previous + (RESAMPLER.next - RESAMPLER.previous) * RESAMPLER.phase as f32;
        RESAMPLER.phase += step;
        while RESAMPLER.phase >= 1.0 {
            RESAMPLER.phase -= 1.0;
            RESAMPLER.previous = RESAMPLER.next;
            RESAMPLER.next = pop();
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
            RESAMPLER = Resampler {
                phase: 0.0,
                previous: 0.0,
                next: 0.0,
            };
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
        AAudioStream_setBufferSizeInFrames(stream, AAudioStream_getFramesPerBurst(stream) * 2);
        if AAudioStream_requestStart(stream) != 0 {
            AAudioStream_close(stream);
            return Err("Could not start audio output");
        }
        *handle = stream as usize;
    }
    Ok(())
}
