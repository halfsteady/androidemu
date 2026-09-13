use emulation_api::{Buttons, CoreError, EmbeddedCore, LoadCore, SystemId};
use nes_adapter::{NesAdapter, DEFAULT_PALETTE};
use nes_core::Nes;

#[path = "../../../core/tests/common/mod.rs"]
mod common;
#[path = "fixtures/legacy_palette.rs"]
mod legacy_palette;

fn rom(pal: bool) -> Vec<u8> {
    let mut rom = common::rom(0, 0);
    rom[9] = u8::from(pal);
    // Pulse audio, controller reads, battery writes, and changing backdrop color.
    // All hardware is exercised through the ROM, not adapter-internal access.
    let setup = [
        0xa9, 1, 0x8d, 0x15, 0x40, // enable pulse 1
        0xa9, 0xbf, 0x8d, 0, 0x40, 0xa9, 0x40, 0x8d, 2, 0x40, 0xa9, 0x08, 0x8d, 3, 0x40,
    ];
    let start = 0x8000u16 + setup.len() as u16;
    let mut program = setup.to_vec();
    program.extend_from_slice(&[
        0xa9,
        1,
        0x8d,
        0x16,
        0x40, // controller strobe
        0xa9,
        0,
        0x8d,
        0x16,
        0x40,
        0xad,
        0x16,
        0x40,
        0x8d,
        0,
        0x60,
        0xe6,
        0, // increment color
        0xa9,
        0x3f,
        0x8d,
        6,
        0x20,
        0xa9,
        0,
        0x8d,
        6,
        0x20,
        0xa5,
        0,
        0x29,
        0x3f,
        0x8d,
        7,
        0x20,
        0x4c,
        start as u8,
        (start >> 8) as u8,
    ]);
    rom[16..16 + program.len()].copy_from_slice(&program);
    rom
}

fn compare(direct: &Nes, adapter: &dyn EmbeddedCore, palette: &[u32; 64], audio: bool) {
    assert_eq!(adapter.snapshot().unwrap(), direct.save_state());
    assert_eq!(adapter.battery_ram(), direct.battery_ram());
    let frame = adapter.video();
    assert_eq!(
        frame.descriptor.required_bytes().unwrap(),
        frame.pixels.len()
    );
    assert_eq!(
        (frame.descriptor.width, frame.descriptor.height),
        Nes::frame_size()
    );
    let expected: Vec<u8> = direct
        .framebuffer()
        .iter()
        .flat_map(|index| {
            let rgb = palette[(index & 63) as usize];
            [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255]
        })
        .collect();
    assert_eq!(frame.pixels, expected);
    if audio {
        assert_eq!(adapter.audio().samples, direct.bus.apu.samples());
    }
}

fn advance(direct: &mut Nes, adapter: &mut dyn EmbeddedCore, n: u16) {
    for (port, bits) in [(0, (n * 37) & 255), (1, (n * 19) & 255)] {
        direct.set_buttons(port, nes_core::Buttons(bits as u8));
        adapter.set_input(port, Buttons(bits)).unwrap();
    }
    direct.step_frame();
    adapter.step_frame().unwrap();
}

#[test]
fn ntsc_and_pal_match_direct_video_audio_input_and_persistence() {
    assert_eq!(DEFAULT_PALETTE, legacy_palette::PALETTE);
    for pal in [false, true] {
        let bytes = rom(pal);
        let mut direct = Nes::new(&bytes).unwrap();
        let mut adapter = NesAdapter::load(&bytes).unwrap();
        assert_eq!(adapter.identity().system, SystemId::Nes);
        assert_eq!(adapter.identity().state_format, "ANES");
        assert_eq!(
            adapter.timing().frames_per_second,
            direct.bus.cart.header.region.frame_rate()
        );
        assert_eq!(adapter.audio().format.sample_rate, 48_000);
        assert_eq!(adapter.audio().format.channels, 1);
        compare(&direct, &adapter, &legacy_palette::PALETTE, false);
        let mut audible = false;
        for n in 0..12 {
            advance(&mut direct, &mut adapter, n);
            audible |= adapter.audio().samples.iter().any(|v| v.abs() > 0.001);
            compare(&direct, &adapter, &legacy_palette::PALETTE, true);
        }
        assert!(audible, "fixture must exercise non-silent audio");
        let saved = adapter.snapshot().unwrap();
        for n in 12..16 {
            advance(&mut direct, &mut adapter, n);
        }
        let future = adapter.snapshot().unwrap();
        let future_audio = adapter.audio().samples.to_vec();
        direct.load_state(&saved).unwrap();
        adapter.load_state(&saved).unwrap();
        assert!(
            adapter.audio().samples.is_empty(),
            "restored audio is not new output"
        );
        compare(&direct, &adapter, &legacy_palette::PALETTE, false);
        for n in 12..16 {
            advance(&mut direct, &mut adapter, n);
            compare(&direct, &adapter, &legacy_palette::PALETTE, true);
        }
        assert_eq!(adapter.snapshot().unwrap(), future);
        assert_eq!(adapter.audio().samples, future_audio);
    }
}

#[test]
fn both_reset_forms_retain_battery_and_palette_without_changing_legacy_state() {
    let bytes = rom(false);
    let mut direct = Nes::new(&bytes).unwrap();
    let mut adapter = NesAdapter::new(&bytes).unwrap();
    let battery: Vec<u8> = (0..direct.battery_ram().unwrap().len())
        .map(|n| n as u8)
        .collect();
    direct.load_battery_ram(&battery).unwrap();
    adapter.load_battery_ram(&battery).unwrap();
    let palette = std::array::from_fn(|i| (i as u32 * 0x030507) & 0xffffff);
    let state = adapter.snapshot().unwrap();
    adapter.set_palette(palette);
    assert_eq!(
        adapter.snapshot().unwrap(),
        state,
        "presentation cannot advance emulation"
    );
    for n in 0..3 {
        advance(&mut direct, &mut adapter, n);
    }
    direct.reset();
    adapter.reset().unwrap();
    compare(&direct, &adapter, &palette, false);
    assert!(adapter.audio().samples.is_empty());
    let battery = direct.battery_ram().unwrap().to_vec();
    direct = Nes::new(&bytes).unwrap();
    direct.load_battery_ram(&battery).unwrap();
    adapter.power_cycle(&bytes).unwrap();
    compare(&direct, &adapter, &palette, false);
}

#[test]
fn rejected_operations_preserve_state_video_audio_and_battery() {
    let bytes = rom(false);
    let mut adapter = NesAdapter::new(&bytes).unwrap();
    adapter.step_frame().unwrap();
    let saved = adapter.snapshot().unwrap();
    let video = adapter.video().pixels.to_vec();
    let audio = adapter.audio().samples.to_vec();
    let battery = adapter.battery_ram().unwrap().to_vec();
    let mut wrong = bytes.clone();
    wrong[100] ^= 1;
    let wrong_state = Nes::new(&wrong).unwrap().save_state();
    let mut corrupt = saved.clone();
    corrupt[100] ^= 1;
    for case in 0..8 {
        let result = match case {
            0 => adapter.load_state(&wrong_state),
            1 => adapter.load_state(&corrupt),
            2 => adapter.load_state(&saved[..20]),
            3 => adapter.load_battery_ram(&[0]),
            4 => adapter.set_input(2, Buttons(0)),
            5 => adapter.set_input(0, Buttons(Buttons::L)),
            6 => adapter.power_cycle(&wrong),
            _ => adapter.power_cycle(&[0]),
        };
        assert!(result.is_err());
        assert_eq!(adapter.snapshot().unwrap(), saved);
        assert_eq!(adapter.video().pixels, video);
        assert_eq!(adapter.audio().samples, audio);
        assert_eq!(adapter.battery_ram().unwrap(), battery);
    }
    assert!(NesAdapter::new(&[0]).is_err());
    let mut no_battery = bytes;
    no_battery[6] &= !2;
    let mut adapter = NesAdapter::new(&no_battery).unwrap();
    assert!(!adapter.capabilities().battery_save);
    assert_eq!(
        adapter.load_battery_ram(&[]),
        Err(CoreError::Unsupported("battery save"))
    );
    adapter.power_cycle(&no_battery).unwrap();
}

#[test]
fn legacy_fixtures_and_original_content_identifiers_survive_the_adapter() {
    let bytes = common::rom(0, 0);
    for saved in [
        &include_bytes!("../../../core/tests/fixtures/preview-v1.state")[..],
        &include_bytes!("../../../core/tests/fixtures/preview-v2.state")[..],
        &include_bytes!("../../../core/tests/fixtures/preview-v2-rendering.state")[..],
    ] {
        let mut direct = Nes::new(&bytes).unwrap();
        let mut adapter = NesAdapter::new(&bytes).unwrap();
        direct.load_state(saved).unwrap();
        adapter.load_state(saved).unwrap();
        compare(&direct, &adapter, &legacy_palette::PALETTE, false);
        for n in 0..3 {
            advance(&mut direct, &mut adapter, n);
            compare(&direct, &adapter, &legacy_palette::PALETTE, true);
        }
    }
    let mut trailing = bytes;
    trailing.extend_from_slice(b"trailing original content");
    trailing[7..16].copy_from_slice(b"DiskDude!");
    let direct = Nes::new(&trailing).unwrap();
    let adapter = NesAdapter::new(&trailing).unwrap();
    assert!(!adapter.header_notes().is_empty());
    assert_ne!(adapter.android_content_id(), adapter.desktop_content_id());
    assert_eq!(
        adapter.android_content_id(),
        format!("{:016x}", direct.bus.cart.header.hash)
    );
    assert_eq!(
        adapter.desktop_content_id(),
        format!("{:016x}", direct.bus.cart.header.identity)
    );
    assert_eq!(
        adapter.header_notes(),
        direct.bus.cart.header.fixes.labels().collect::<Vec<_>>()
    );
}

#[test]
fn descriptor_rejects_overflow_invalid_crop_and_short_stride() {
    let adapter = NesAdapter::new(&rom(false)).unwrap();
    let valid = adapter.video().descriptor;
    for invalid in [
        emulation_api::VideoDescriptor {
            width: usize::MAX,
            ..valid
        },
        emulation_api::VideoDescriptor {
            height: usize::MAX,
            ..valid
        },
        emulation_api::VideoDescriptor {
            stride_bytes: 1,
            ..valid
        },
        emulation_api::VideoDescriptor {
            pixel_aspect: (0, 1),
            ..valid
        },
        emulation_api::VideoDescriptor {
            visible: emulation_api::Rect {
                x: usize::MAX,
                ..valid.visible
            },
            ..valid
        },
    ] {
        assert!(invalid.required_bytes().is_err());
    }
}
