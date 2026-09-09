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

#[test]
fn snapshot_preserves_ppu_charge_and_a_pending_apu_acknowledgement() {
    let mut nes = Nes::new(&common::rom(0, 0)).unwrap();
    nes.bus.ppu.write_register(0x2002, 0xff, nes.bus.cart.mapper.as_mut());
    for _ in 0..341 * 262 * 19 { nes.bus.ppu.tick(nes.bus.cart.mapper.as_mut()); }
    for _ in 0..30000 { nes.bus.apu.tick(); }
    assert_ne!(nes.bus.apu.read_register(0x4015) & 0x40, 0);
    let saved = nes.save_state();
    assert_eq!(&saved[..8], b"ANES\x02\0\0\0");
    let advance = |nes: &mut Nes| {
        for _ in 0..2 { nes.bus.apu.tick(); }
        assert!(!nes.bus.apu.irq_line());
        for _ in 0..341 * 262 * 2 { nes.bus.ppu.tick(nes.bus.cart.mapper.as_mut()); }
        assert_eq!(nes.bus.ppu.read_register(0x2000, nes.bus.cart.mapper.as_mut()), 0);
    };
    advance(&mut nes);
    let expected = nes.save_state();
    nes.load_state(&saved).unwrap();
    advance(&mut nes);
    assert!(nes.save_state() == expected, "restored timing must replay identically");
}

#[test]
fn snapshot_preserves_distinct_internal_and_dma_bus_values() {
    let mut rom = common::rom(0, 0);
    rom[16 + 0x4000] = 0x20;
    let mut nes = Nes::new(&rom).unwrap();
    nes.bus.write(0, 0);
    nes.bus.apu.write_register(0x4015, 0x10);
    assert_eq!(nes.bus.read(0x4015), 0);
    let saved = nes.save_state();
    nes.bus.read(0x4000); // Drives DMA's bit 5 onto the CPU's internal bus.
    assert_eq!(nes.bus.read(0x4015), 0x20);
    nes.load_state(&saved).unwrap();
    assert_eq!(nes.bus.read(0x4015), 0);
    assert_eq!(nes.bus.read(0x4000), 0x20);
}

#[test]
fn malformed_version_two_extensions_are_rejected_transactionally() {
    let mut nes = Nes::new(&common::rom(0, 0)).unwrap();
    let saved = nes.save_state();
    let mut cases = Vec::new();
    let mut unknown = saved[..saved.len() - 8].to_vec();
    unknown[4] = 3;
    cases.push(unknown);
    let mut reserved = saved[..saved.len() - 8].to_vec();
    reserved[5] = 1;
    cases.push(reserved);
    // Keep a correct checksum, so decoding itself must reject missing fields.
    for missing in [1, 2, 65, 66] {
        cases.push(saved[..saved.len() - 8 - missing].to_vec());
    }
    let mut invalid_bool = saved[..saved.len() - 8].to_vec();
    *invalid_bool.last_mut().unwrap() = 2;
    cases.push(invalid_bool);
    let mut trailing = saved[..saved.len() - 8].to_vec();
    trailing.push(0);
    cases.push(trailing);
    for mut bad in cases {
        let hash = bad.iter().fold(0xcbf29ce484222325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3));
        bad.extend_from_slice(&hash.to_le_bytes());
        assert!(nes.load_state(&bad).is_err());
        assert!(nes.save_state() == saved);
    }
}
