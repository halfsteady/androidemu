use emulation_api::{Buttons, EmbeddedCore, LoadCore};
use emulation_runtime::{
    history::Rewind,
    scheduling::{run_batch, FrameClock},
};
use nes_adapter::NesAdapter;

#[test]
fn clock_paces_and_resets_after_pause_or_stall() {
    let mut clock = FrameClock::new();
    assert!(clock.due(0, 50.0, true));
    assert!(!clock.due(19_999_999, 50.0, true));
    assert!(clock.due(20_000_000, 50.0, true));
    assert!(clock.due(1_000_000_000, 50.0, true));
    assert!(!clock.due(1_000_000_001, 50.0, true));
    assert!(!clock.due(1_000_000_002, 50.0, false));
    assert!(clock.due(1_000_000_003, 50.0, true));
}

#[test]
fn nes_fast_forward_rewind_exhaustion_and_audio_resume() {
    let mut rom = vec![0; 16 + 16384];
    rom[..4].copy_from_slice(b"NES\x1a");
    rom[4] = 1;
    rom[16..19].copy_from_slice(&[0x4c, 0, 0x80]);
    rom[16 + 0x3ffd] = 0x80;
    let mut core = NesAdapter::load(&rom).unwrap();
    let mut reference = NesAdapter::load(&rom).unwrap();
    let mut history = Rewind::new(8 * 1024 * 1024);
    history.push(&core.snapshot().unwrap());
    let inputs = [Buttons(1), Buttons(0)];
    assert_eq!(
        run_batch(&mut core, Some(&mut history), 8, inputs, |_| panic!(
            "fast-forward audio"
        ))
        .unwrap(),
        8
    );
    for _ in 0..8 {
        reference.set_input(0, inputs[0]).unwrap();
        reference.step_frame().unwrap();
    }
    assert_eq!(core.snapshot().unwrap(), reference.snapshot().unwrap());
    assert_eq!(history.depth(), 8);
    assert_eq!(
        run_batch(&mut core, Some(&mut history), -20, inputs, |_| panic!(
            "rewind audio"
        ))
        .unwrap(),
        8
    );
    let oldest = core.snapshot().unwrap();
    assert_eq!(
        run_batch(&mut core, Some(&mut history), -20, inputs, |_| panic!()).unwrap(),
        0
    );
    assert_eq!(core.snapshot().unwrap(), oldest);
    let mut calls = 0;
    run_batch(&mut core, Some(&mut history), 1, inputs, |audio| {
        assert!(!audio.samples.is_empty());
        calls += 1;
    })
    .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(history.depth(), 1);
}
