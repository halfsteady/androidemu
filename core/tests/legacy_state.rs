mod common;
use nes_core::Nes;

#[test]
fn both_released_snapshot_formats_stay_compatible() {
    for (saved, length, hash) in [
        (&include_bytes!("fixtures/preview-v1.state")[..], 96881, 0xcbce44fcd468ddf1),
        (&include_bytes!("fixtures/preview-v2.state")[..], 96947, 0xdf65d2c299a733df),
    ] {
        let actual_hash = saved.iter().fold(0xcbf29ce484222325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3));
        assert_eq!(saved.len(), length);
        assert_eq!(actual_hash, hash);
        let mut nes = Nes::new(&common::rom(0, 0)).unwrap();
        nes.load_state(saved).unwrap();
        let migrated = nes.save_state();
        // Migration retains every old field, including timer phases from the
        // released writer. New power-on defaults must not replace saved values.
        assert!(migrated[8..saved.len() - 8] == saved[8..saved.len() - 8]);
        for _ in 0..3 { nes.step_frame(); }
        let expected = nes.save_state();
        nes.load_state(&migrated).unwrap();
        assert!(nes.save_state() == migrated);
        for _ in 0..3 { nes.step_frame(); }
        assert!(nes.save_state() == expected, "v{} migration replays identically", saved[4]);
    }
}

#[test]
fn released_v2_rendering_state_keeps_the_next_sprite_hit_at_its_screen_position() {
    let saved = include_bytes!("fixtures/preview-v2-rendering.state");
    let hash = saved.iter().fold(0xcbf29ce484222325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3));
    assert_eq!(saved.len(), 96947);
    assert_eq!(hash, 0x92146647353737b2);
    let mut nes = Nes::new(&common::rom(0, 0)).unwrap();
    nes.load_state(saved).unwrap();
    assert_eq!((nes.bus.ppu.scanline, nes.bus.ppu.dot), (21, 81));
    while nes.bus.ppu.dot < 120 { nes.step(); }
    assert_eq!(nes.bus.ppu.status & 0x40, 0);
    while nes.bus.ppu.dot < 140 { nes.step(); }
    assert_eq!(nes.bus.ppu.status & 0x40, 0x40);
    assert_eq!(nes.framebuffer()[21 * 256 + 128], 0x32);
}
