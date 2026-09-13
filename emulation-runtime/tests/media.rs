use emulation_api::*;
use emulation_runtime::*;
use synthetic_core::SyntheticCore;

#[test]
fn padded_growing_and_shrinking_frames_copy_without_padding_or_stale_pixels() {
    let mut core = SyntheticCore::new();
    let mut sizes = Vec::new();
    for _ in 0..9 {
        core.step_frame().unwrap();
        let frame = core.video();
        let d = frame.descriptor;
        sizes.push((d.width, d.height));
        let stride = d.width * 4 + 12;
        let mut destination = vec![0xee; stride * d.height];
        copy_rgba(&frame, &mut destination, stride).unwrap();
        for y in 0..d.height {
            for x in 0..d.width {
                assert_eq!(
                    &destination[y * stride + x * 4..y * stride + x * 4 + 4],
                    &[x as u8, y as u8, 0, 255]
                );
            }
            assert!(destination[y * stride + d.width * 4..(y + 1) * stride]
                .iter()
                .all(|b| *b == 0xee));
        }
    }
    assert_eq!(&sizes[..3], &[(512, 478), (320, 240), (256, 224)]);
}

#[test]
fn invalid_video_never_writes_the_destination() {
    let core = SyntheticCore::new();
    let frame = core.video();
    let mut out = vec![0xee; 32];
    assert!(copy_rgba(&frame, &mut out, 4).is_err());
    assert_eq!(out, vec![0xee; 32]);
    let short = VideoFrame {
        descriptor: frame.descriptor,
        pixels: &frame.pixels[..10],
    };
    assert!(validate_video(&short).is_err());
    let huge = VideoFrame {
        descriptor: VideoDescriptor {
            height: usize::MAX,
            ..frame.descriptor
        },
        pixels: frame.pixels,
    };
    assert!(validate_video(&huge).is_err());
}

#[test]
fn stereo_frames_keep_channels_and_queue_duration_through_resampling() {
    let mut core = SyntheticCore::new();
    core.step_frame().unwrap();
    let audio = core.audio();
    assert_eq!(audio_frames(&audio).unwrap(), 800);
    assert_eq!(
        audio_queue_bytes(audio.format, 60.0, 2.0).unwrap(),
        1600 * 2 * 4
    );
    let mut frames = audio.samples.chunks_exact(2).map(|s| [s[0], s[1]]);
    let mut resampler = Resampler::new();
    let mut out = vec![0.0; 700 * 2];
    assert!(!resampler.render(&mut out, 2, 48000.0 / 44100.0, || frames.next()));
    for frame in out.chunks_exact(2).skip(3) {
        assert_eq!(frame, &[0.25, -0.5]);
    }
    let mut held = [0.0; 16];
    assert!(resampler.render(&mut held, 2, 1.0, || None));
    for frame in held.chunks_exact(2) {
        assert_eq!(frame, &[0.25, -0.5]);
    }
    resampler = Resampler::new();
    assert!(resampler.render(&mut held, 2, 1.0, || None));
    assert_eq!(held, [0.0; 16]);
    assert!(audio_frames(&AudioFrame {
        format: audio.format,
        samples: &[0.0]
    })
    .is_err());
    assert!(audio_queue_bytes(audio.format, f64::NAN, 2.0).is_err());
}

#[test]
fn mono_interpolation_matches_the_legacy_callback_sequence() {
    let input: Vec<f32> = (0..800).map(|i| (i as f32 / 20.0).sin()).collect();
    let mut expected = vec![0.0; 500];
    let (mut phase, mut previous, mut next, mut at) = (0.0, 0.0, 0.0, 0);
    let step = 48000.0 / 44100.0;
    for value in &mut expected {
        *value = previous + (next - previous) * phase as f32;
        phase += step;
        while phase >= 1.0 {
            phase -= 1.0;
            previous = next;
            next = input[at];
            at += 1;
        }
    }
    let mut out = vec![0.0; 500];
    let mut source = input.iter().map(|v| [*v, 0.0]);
    assert!(!Resampler::new().render(&mut out, 1, step, || source.next()));
    assert_eq!(out, expected);
}

#[test]
fn unsupported_capabilities_and_inputs_are_real_errors() {
    let mut core = SyntheticCore::new();
    assert!(!core.capabilities().save_states);
    assert!(core.snapshot().is_err());
    assert!(core.load_state(&[0]).is_err());
    assert!(core.load_battery_ram(&[0]).is_err());
    assert!(core.set_input(1, Buttons(0)).is_err());
    assert!(core.set_input(0, Buttons(Buttons::B)).is_err());
    core.set_input(0, Buttons(Buttons::L | Buttons::R)).unwrap();
    core.step_frame().unwrap();
}
