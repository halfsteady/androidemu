//! Silent on-device check of the real audio backend, with NES-rate production.
//! Build for aarch64-linux-android, push to /data/local/tmp, then run with an
//! optional positive frame count per session (default 1800). Two sessions check
//! both startup and reopen. No ROM, app launch, or system setting change needed.

#[cfg(target_os = "android")]
#[path = "../src/audio.rs"]
mod audio;

#[cfg(target_os = "android")]
fn main() {
    use std::time::{Duration, Instant};

    let frames = std::env::args().nth(1).map_or(1800, |value| {
        value.parse::<u32>().ok().filter(|&n| n > 0).expect("expected a positive frame count")
    });
    const FRAME_RATE: f64 = 60.0988;
    let frame_time = Duration::from_secs_f64(1.0 / FRAME_RATE);
    let silence = [0.0; 800];
    for session in 1..=2 {
        audio::set_frame_rate(FRAME_RATE as f32);
        audio::set_playing(true).expect("could not open audio output");
        let start = Instant::now();
        for frame in 0..frames {
            let samples = (48000.0 / FRAME_RATE * (frame + 1) as f64).floor() as usize
                - (48000.0 / FRAME_RATE * frame as f64).floor() as usize;
            audio::push(&silence[..samples]);
            if frame % 60 == 59 || frame + 1 == frames {
                let (queue, buffer, gaps) = audio::stats();
                println!(
                    "session={session} frames={} elapsed={:.3} queue_ms={queue:.1} \
                     buffer_ms={buffer:.1} target_ms={:.1} source_gaps={gaps} output_gaps={}",
                    frame + 1, start.elapsed().as_secs_f64(), audio::target_ms(), audio::output_underruns(),
                );
            }
            let deadline = start + frame_time * (frame + 1);
            std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        }
        audio::set_playing(false).expect("could not close audio output");
    }
}

#[cfg(not(target_os = "android"))]
fn main() {
    eprintln!("Build audio_probe for Android and run it on the device; see docs/RASPBERRY-PI.md.");
    std::process::exit(2);
}
