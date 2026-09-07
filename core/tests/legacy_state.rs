mod common;
use nes_core::Nes;
#[test]
fn first_preview_snapshot_format_stays_compatible() {
    let mut nes = Nes::new(&common::rom(0, 0)).unwrap();
    for _ in 0..1234 { nes.step(); }
    let saved = nes.save_state();
    let hash = saved.iter().fold(0xcbf29ce484222325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3));
    assert_eq!(saved.len(), 96881);
    assert_eq!(hash, 0xcbce44fcd468ddf1);
    nes.load_state(&saved).unwrap();
    assert_eq!(nes.save_state(), saved);
}
