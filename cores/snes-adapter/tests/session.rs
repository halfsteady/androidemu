use emulation_api::{Buttons, EmbeddedCore};
use emulation_runtime::{history::Rewind, scheduling::run_batch};
use snes_adapter::SnesAdapter;

fn rom(battery: bool) -> Vec<u8> {
    let mut bytes = vec![0; 32768];
    // Change CGRAM and battery SRAM every loop, without an enhancement chip.
    let program = [
        0x78, 0xa9, 0x0f, 0x8d, 0x00, 0x21, 0xe6, 0x00, 0xa5, 0x00, 0x9c, 0x21, 0x21, 0x8d, 0x22,
        0x21, 0x9c, 0x22, 0x21, 0xa9, 0xa7, 0x8f, 0x00, 0x00, 0x70, 0x80, 0xeb,
    ];
    bytes[..program.len()].copy_from_slice(&program);
    bytes[0x7fc0..0x7fd5].copy_from_slice(b"EMULIA GENERATED TEST");
    bytes[0x7fd5] = 0x20;
    bytes[0x7fd6] = if battery { 2 } else { 0 };
    bytes[0x7fd7] = 5;
    bytes[0x7fd8] = u8::from(battery);
    bytes[0x7fd9] = 1;
    bytes[0x7fdc] = 0xff;
    bytes[0x7fdd] = 0xff;
    bytes[0x7ffd] = 0x80;
    bytes
}
#[test]
fn restore_replays_video_audio_and_rewind_holds_at_exhaustion() {
    let mut core = SnesAdapter::new(&rom(true)).unwrap();
    core.set_input(1, Buttons(Buttons::X | Buttons::L)).unwrap();
    for _ in 0..3 {
        core.step_frame().unwrap();
    }
    let saved = core.snapshot().unwrap();
    let preview = core.video().pixels.to_vec();
    for _ in 0..3 {
        core.step_frame().unwrap();
    }
    let expected = core.snapshot().unwrap();
    let picture = core.video().pixels.to_vec();
    let audio = core.audio().samples.to_vec();
    core.load_state(&saved).unwrap();
    assert_eq!(core.video().pixels, preview);
    assert!(core.audio().samples.is_empty());
    for _ in 0..3 {
        core.step_frame().unwrap();
    }
    assert_eq!(core.snapshot().unwrap(), expected);
    assert_eq!(core.video().pixels, picture);
    assert_eq!(core.audio().samples, audio);
    let mut history = Rewind::new(64 * 1024 * 1024);
    history.push(&core.snapshot().unwrap());
    assert_eq!(
        run_batch(
            &mut core,
            Some(&mut history),
            8,
            [Buttons(Buttons::R), Buttons(Buttons::Y)],
            |_| panic!("fast-forward audio")
        )
        .unwrap(),
        8
    );
    assert_eq!(
        run_batch(
            &mut core,
            Some(&mut history),
            -15,
            [Buttons(0); 2],
            |_| panic!()
        )
        .unwrap(),
        8
    );
    assert_eq!(
        run_batch(
            &mut core,
            Some(&mut history),
            -15,
            [Buttons(0); 2],
            |_| panic!()
        )
        .unwrap(),
        0
    );
}
#[test]
fn battery_is_current_before_periodic_callback_and_bad_import_is_isolated() {
    let mut core = SnesAdapter::new(&rom(true)).unwrap();
    let bytes = vec![0x55; 2048];
    core.load_battery_ram(&bytes).unwrap();
    assert_eq!(core.battery_ram().unwrap(), bytes);
    core.step_frame().unwrap();
    assert_eq!(core.battery_ram().unwrap()[0], 0xa7);
    let before = core.snapshot().unwrap();
    let video = core.video().pixels.to_vec();
    assert!(core.load_battery_ram(&[1; 3]).is_err());
    assert_eq!(core.snapshot().unwrap(), before);
    assert_eq!(core.video().pixels, video);
    let saved_battery = core.battery_ram().unwrap().to_vec();
    core.reset().unwrap();
    assert_eq!(core.battery_ram().unwrap(), saved_battery);
}
#[test]
fn corrupt_wrong_content_and_wrong_version_states_do_not_change_the_session() {
    let bytes = rom(false);
    let mut core = SnesAdapter::new(&bytes).unwrap();
    core.step_frame().unwrap();
    let before = core.snapshot().unwrap();
    let mut other = bytes.clone();
    other[100] = 1;
    let foreign = SnesAdapter::new(&other).unwrap().snapshot().unwrap();
    let mut corrupt = before.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    let mut version = before.clone();
    version[4] = 2;
    for invalid in [vec![], vec![0; 10], foreign, corrupt, version] {
        assert!(core.load_state(&invalid).is_err());
        assert_eq!(core.snapshot().unwrap(), before);
    }
    let mut copier = vec![0; 512];
    copier.extend_from_slice(&bytes);
    assert_eq!(
        SnesAdapter::content_id(&bytes).unwrap(),
        SnesAdapter::content_id(&copier).unwrap()
    );
    let mut chip = bytes;
    chip[0x7fd6] = 0x13;
    assert!(SnesAdapter::new(&chip).is_err());
    assert!(SnesAdapter::new(&[0; 32768]).is_err());
}

#[test]
fn hirom_pal_timing_and_input_validation() {
    let lo = rom(false);
    let mut hi = vec![0; 65536];
    hi[0x8000..].copy_from_slice(&lo);
    hi[0xffd5] = 0x21;
    hi[0xffd7] = 6;
    hi[0xffd9] = 2; // PAL region
    let mut core = SnesAdapter::new(&hi).unwrap();
    assert!((49.0..51.0).contains(&core.timing().frames_per_second));
    core.step_frame().unwrap();
    assert_eq!(core.audio().format.channels, 2);
    assert!(!core.audio().samples.is_empty());
    assert_eq!(
        core.video().descriptor.required_bytes().unwrap(),
        core.video().pixels.len()
    );
    let before = core.snapshot().unwrap();
    assert!(core.set_input(2, Buttons(0)).is_err());
    assert!(core.set_input(0, Buttons(0x1000)).is_err());
    assert_eq!(core.snapshot().unwrap(), before);
    assert!(core.battery_ram().is_none());
}
