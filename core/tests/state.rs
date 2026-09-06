mod common;
use nes_core::{cpu::Bus, Buttons, Nes};
#[test]
fn mid_frame_snapshot_replays_identically_for_every_tier_one_mapper() {
    for mapper in [0, 1, 2, 3, 4, 7] {
        let mut nes = Nes::new(&common::rom(mapper, 0)).unwrap();
        nes.bus.ppu.mask = 0x1e;
        nes.bus.write(0x4015, 1);
        nes.bus.write(0x4000, 0x9f);
        nes.bus.write(0x4002, 80);
        nes.bus.write(0x4003, 8);
        nes.set_buttons(0, Buttons(0x89));
        nes.bus.controllers[0].write_strobe(1);
        nes.bus.controllers[0].write_strobe(0);
        nes.bus.controllers[0].read();
        for _ in 0..1234 {
            nes.step();
        }
        let saved = nes.save_state();
        for _ in 0..3 {
            nes.step_frame();
        }
        let expected = nes.save_state();
        nes.load_state(&saved).unwrap();
        assert_eq!(nes.save_state(), saved);
        for _ in 0..3 {
            nes.step_frame();
        }
        assert_eq!(nes.save_state(), expected, "mapper {mapper}");
    }
}
#[test]
fn bad_states_are_rejected_without_mutation() {
    let mut nes = Nes::new(&common::rom(0, 0)).unwrap();
    let saved = nes.save_state();
    for n in [0, 7, 25, saved.len() - 1] {
        assert!(nes.load_state(&saved[..n]).is_err());
        assert_eq!(nes.save_state(), saved);
    }
    let mut bad = saved.clone();
    bad[30] ^= 1;
    assert!(nes.load_state(&bad).is_err());
    let mut other_rom = common::rom(0, 0);
    other_rom[100] ^= 1;
    assert!(nes
        .load_state(&Nes::new(&other_rom).unwrap().save_state())
        .is_err());
    assert_eq!(nes.save_state(), saved);
}
#[test]
fn battery_save_requires_exact_size() {
    let mut nes = Nes::new(&common::rom(1, 0)).unwrap();
    nes.bus.write(0x6000, 93);
    let ram = nes.battery_ram().unwrap().to_vec();
    let mut fresh = Nes::new(&common::rom(1, 0)).unwrap();
    fresh.load_battery_ram(&ram).unwrap();
    assert_eq!(fresh.bus.read(0x6000), 93);
    assert!(fresh.load_battery_ram(&ram[..10]).is_err());
}

#[test]
fn state_rejects_same_payload_with_different_mirroring() {
    let mut rom = common::rom(0, 0);
    let mut nes = Nes::new(&rom).unwrap();
    let before = nes.save_state();
    rom[6] |= 1;
    let other = Nes::new(&rom).unwrap();
    assert_eq!(nes.bus.cart.header.hash, other.bus.cart.header.hash);
    assert!(nes.load_state(&other.save_state()).is_err());
    assert_eq!(nes.save_state(), before);
}
