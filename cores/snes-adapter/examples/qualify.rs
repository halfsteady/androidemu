//! Local-ROM qualification. ROMs, rendered frames and states stay outside the repository.
use emulation_api::{Buttons, EmbeddedCore};
use emulation_runtime::{history::Rewind, scheduling::run_batch};
use sha2::{Digest, Sha256};
use snes_adapter::SnesAdapter;
use std::{fs, io::Write, path::Path, time::Instant};

fn capture(core: &SnesAdapter, out: &Path, name: &str) {
    let video = core.video();
    let d = video.descriptor;
    let mut file = fs::File::create(out.join(format!("{name}.ppm"))).unwrap();
    write!(file, "P6\n{} {}\n255\n", d.width, d.height).unwrap();
    for row in video.pixels.chunks(d.stride_bytes).take(d.height) {
        for pixel in row[..d.width * 4].as_chunks::<4>().0 {
            file.write_all(&pixel[..3]).unwrap();
        }
    }
    fs::write(out.join(format!("{name}.state")), core.snapshot().unwrap()).unwrap();
}
fn advance(core: &mut SnesAdapter, frames: usize, bits: u16) {
    core.set_input(0, Buttons(bits)).unwrap();
    for _ in 0..frames {
        core.step_frame().unwrap();
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert!(
        args.len() >= 4,
        "qualify ROM OUTPUT_DIRECTORY SCRIPT [INITIAL_STATE]"
    );
    let rom = fs::read(&args[1]).unwrap();
    let out = Path::new(&args[2]);
    fs::create_dir_all(out).unwrap();
    let mut core = SnesAdapter::new(&rom).unwrap();
    println!(
        "content={} fps={:.6}",
        core.content_identity().value,
        core.timing().frames_per_second
    );
    if let Some(state) = args.get(4).filter(|s| !s.starts_with("--")) {
        core.load_state(&fs::read(state).unwrap()).unwrap();
    }
    // Each line: frame count, decimal button mask, checkpoint name.
    for line in fs::read_to_string(&args[3])
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let fields: Vec<_> = line.split_whitespace().collect();
        advance(
            &mut core,
            fields[0].parse().unwrap(),
            fields[1].parse().unwrap(),
        );
        capture(&core, out, fields[2]);
        println!(
            "checkpoint={} video={:x}",
            fields[2],
            Sha256::digest(core.video().pixels)
        );
    }
    if args.iter().any(|a| a == "--capture-only") {
        return;
    }
    core.set_input(0, Buttons(0)).unwrap();
    let start = core.snapshot().unwrap();
    let initial_video = core.video().pixels.to_vec();
    let replay = |core: &mut SnesAdapter| {
        let mut video = Sha256::new();
        let mut audio = Sha256::new();
        let mut peaks = [0.0_f32; 2];
        let mut stereo_differences = 0;
        for frame in 0..120 {
            core.set_input(
                0,
                Buttons(if frame < 60 {
                    Buttons::RIGHT | Buttons::B
                } else {
                    0
                }),
            )
            .unwrap();
            core.step_frame().unwrap();
            video.update(core.video().pixels);
            for sample in core.audio().samples {
                assert!(sample.is_finite());
                audio.update(sample.to_le_bytes());
            }
            for pair in core.audio().samples.as_chunks::<2>().0 {
                for ch in 0..2 {
                    peaks[ch] = peaks[ch].max(pair[ch].abs());
                }
                stereo_differences += usize::from(pair[0] != pair[1]);
            }
        }
        (
            core.snapshot().unwrap(),
            video.finalize(),
            audio.finalize(),
            peaks,
            stereo_differences,
        )
    };
    let expected = replay(&mut core);
    core.load_state(&start).unwrap();
    assert_eq!(core.video().pixels, initial_video);
    assert!(core.audio().samples.is_empty());
    let actual = replay(&mut core);
    assert!(actual == expected, "restored gameplay diverged");
    println!(
        "replay=PASS frames=120 stereo_peaks={:?} differing_pairs={}",
        actual.3, actual.4
    );
    let battery = core.battery_ram().unwrap().to_vec();
    let mut reopened = SnesAdapter::new(&rom).unwrap();
    reopened.load_state(&start).unwrap();
    assert_eq!(reopened.snapshot().unwrap(), start);
    println!("fresh_core_state_restore=PASS");
    reopened.load_battery_ram(&battery).unwrap();
    assert_eq!(reopened.battery_ram().unwrap(), battery);
    println!(
        "battery_roundtrip=PASS bytes={} (not a completed in-game save)",
        battery.len()
    );
    let mut history = Rewind::new(64 * 1024 * 1024);
    core.load_state(&start).unwrap();
    history.push(&start);
    let now = Instant::now();
    for _ in 0..600 {
        run_batch(&mut core, Some(&mut history), 1, [Buttons(0); 2], |_| {}).unwrap();
    }
    println!(
        "history_capture_fps={:.2} frames=600 depth={}",
        600.0 / now.elapsed().as_secs_f64(),
        history.depth()
    );
    let depth = history.depth();
    assert_eq!(
        run_batch(
            &mut core,
            Some(&mut history),
            -(depth as i32 + 10),
            [Buttons(0); 2],
            |_| panic!()
        )
        .unwrap(),
        depth
    );
    assert_eq!(
        run_batch(
            &mut core,
            Some(&mut history),
            -1,
            [Buttons(0); 2],
            |_| panic!()
        )
        .unwrap(),
        0
    );
    let state = core.snapshot().unwrap();
    let now = Instant::now();
    for _ in 0..150 {
        run_batch(&mut core, Some(&mut history), 4, [Buttons(0); 2], |_| {
            panic!("fast-forward audio")
        })
        .unwrap();
    }
    println!(
        "fast_forward_fps={:.2} frames=600",
        600.0 / now.elapsed().as_secs_f64()
    );
    core.load_state(&state).unwrap();
    let mut sound = 0;
    run_batch(&mut core, None, 1, [Buttons(0); 2], |audio| {
        sound += audio.samples.len()
    })
    .unwrap();
    assert!(sound > 0);
    println!(
        "rewind_exhaustion=PASS fast_forward_mute=PASS normal_audio_resume=PASS samples={sound}"
    );
}
