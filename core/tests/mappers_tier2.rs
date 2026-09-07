//! Tier-2 boards: MMC2/MMC4, Color Dreams, Sunsoft FME-7 and Codemasters.
//!
//! The ROM images fill every 8 KB PRG bank with its own index and every 1 KB CHR
//! bank with its own index, so a read tells you which bank answered.

use nes_core::cart::{Cartridge, Mirroring};

/// Like `common::rom`, but able to carry a mapper number above 15 — which needs
/// the high nibble of flags 7 as well as flags 6.
fn rom(mapper: u16, chr: u8) -> Vec<u8> {
    let mut rom = vec![0; 16 + 4 * 16384 + chr as usize * 8192];
    rom[..4].copy_from_slice(b"NES\x1a");
    rom[4] = 4;
    rom[5] = chr;
    rom[6] = (((mapper & 0x0f) as u8) << 4) | 2;
    rom[7] = (mapper & 0xf0) as u8;
    for bank in 0..8 {
        rom[16 + bank * 8192..16 + (bank + 1) * 8192].fill(bank as u8);
    }
    for bank in 0..chr as usize * 8 {
        rom[16 + 65536 + bank * 1024..16 + 65536 + (bank + 1) * 1024].fill(bank as u8);
    }
    for bank in 0..4 {
        let start = 16 + bank * 16384;
        rom[start..start + 3].copy_from_slice(&[0x4c, 0, 0x80]);
        rom[start + 0x3ffc..start + 0x4000].copy_from_slice(&[0, 0x80, 0, 0x80]);
    }
    rom
}

#[test]
fn color_dreams_swaps_the_nibbles_gxrom_uses() {
    let mut c = Cartridge::load(&rom(11, 4)).unwrap();
    let m = c.mapper.as_mut();
    // PRG in the low bits: bank 0 puts 8 KB banks 0-3 across the window.
    m.cpu_write(0x8000, 0x00);
    assert_eq!(m.cpu_read(0x8100), Some(0));
    assert_eq!(m.cpu_read(0xc100), Some(2));
    // Bank 1 is the second 32 KB, so banks 4-7.
    m.cpu_write(0x8000, 0x01);
    assert_eq!(m.cpu_read(0x8100), Some(4));
    assert_eq!(m.cpu_read(0xe100), Some(7));
    // CHR in the high bits, 8 KB at a time.
    m.cpu_write(0x8000, 0x00);
    assert_eq!(m.ppu_read(0), 0);
    m.cpu_write(0x8000, 0x10);
    assert_eq!(m.ppu_read(0), 8);
    assert_eq!(m.ppu_read(0x1fff), 15);
    // Nothing on this board touches mirroring.
    assert_eq!(m.mirroring(), Mirroring::Horizontal);
}

#[test]
fn codemasters_banks_like_uxrom_and_latches_single_screen() {
    let mut c = Cartridge::load(&rom(71, 4)).unwrap();
    let m = c.mapper.as_mut();
    m.cpu_write(0xc000, 1);
    assert_eq!(m.cpu_read(0x8100), Some(2));
    // The top window is fixed to the last bank whatever is selected.
    assert_eq!(m.cpu_read(0xc100), Some(6));
    assert_eq!(m.cpu_read(0xe100), Some(7));

    // Until $9000 is written the header's mirroring stands.
    assert_eq!(m.mirroring(), Mirroring::Horizontal);
    m.cpu_write(0x9000, 0x10);
    assert_eq!(m.mirroring(), Mirroring::SingleScreenHi);
    m.cpu_write(0x9000, 0x00);
    assert_eq!(m.mirroring(), Mirroring::SingleScreenLo);
    // A write in the mirroring range must not also move the PRG bank.
    assert_eq!(m.cpu_read(0x8100), Some(2));
}

#[test]
fn mmc2_fixes_three_banks_and_switches_one() {
    let mut c = Cartridge::load(&rom(9, 4)).unwrap();
    let m = c.mapper.as_mut();
    m.cpu_write(0xa000, 0);
    assert_eq!(m.cpu_read(0x8100), Some(0));
    // The last three 8 KB banks are wired down.
    assert_eq!(m.cpu_read(0xa100), Some(5));
    assert_eq!(m.cpu_read(0xc100), Some(6));
    assert_eq!(m.cpu_read(0xe100), Some(7));
    m.cpu_write(0xa000, 3);
    assert_eq!(m.cpu_read(0x8100), Some(3));
    assert_eq!(m.cpu_read(0xa100), Some(5), "the fixed banks must not move");
}

#[test]
fn the_chr_latch_switches_after_the_fetch_that_triggered_it() {
    let mut c = Cartridge::load(&rom(9, 4)).unwrap();
    let m = c.mapper.as_mut();
    // $0000-$0FFF reads bank 0 once latched to $FD, bank 1 once latched to $FE.
    m.cpu_write(0xb000, 0);
    m.cpu_write(0xc000, 1);
    // Power-on sits on the $FE register, so 4 KB bank 1: 1 KB banks 4-7.
    assert_eq!(m.ppu_read(0), 4);

    // The fetch of the trigger tile is itself served by the outgoing bank —
    // getting this backwards swaps a frame early and makes the sprite flicker.
    assert_eq!(m.ppu_read(0x0fd8), 7, "the triggering fetch reads the old bank");
    assert_eq!(m.ppu_read(0), 0, "and the next one reads the new bank");

    // And back again.
    assert_eq!(m.ppu_read(0x0fe8), 3);
    assert_eq!(m.ppu_read(0), 4);

    // The high window has its own pair of registers and its own latch.
    m.cpu_write(0xd000, 2);
    m.cpu_write(0xe000, 3);
    assert_eq!(m.ppu_read(0x1000), 12, "the $FE register, 4 KB bank 3");
    m.ppu_read(0x1fd8);
    assert_eq!(m.ppu_read(0x1000), 8, "now the $FD register, 4 KB bank 2");
    assert_eq!(m.ppu_read(0), 4, "the low window's latch is untouched");
}

#[test]
fn mmc2_latches_on_one_address_and_mmc4_on_a_range() {
    // MMC4 games fetch the attribute byte of the same tile, so the board has to
    // react to the whole eight-byte range or it never latches at all.
    let mut c = Cartridge::load(&rom(10, 4)).unwrap();
    let m = c.mapper.as_mut();
    m.cpu_write(0xb000, 0);
    m.cpu_write(0xc000, 1);
    assert_eq!(m.ppu_read(0), 4);
    m.ppu_read(0x0fdb);
    assert_eq!(m.ppu_read(0), 0, "MMC4 latches across the range");

    let mut c = Cartridge::load(&rom(9, 4)).unwrap();
    let m = c.mapper.as_mut();
    m.cpu_write(0xb000, 0);
    m.cpu_write(0xc000, 1);
    assert_eq!(m.ppu_read(0), 4);
    m.ppu_read(0x0fdb);
    assert_eq!(m.ppu_read(0), 4, "MMC2 reacts to the exact address only");
}

#[test]
fn mmc4_switches_sixteen_kilobytes_and_has_working_ram() {
    let mut c = Cartridge::load(&rom(10, 4)).unwrap();
    let m = c.mapper.as_mut();
    m.cpu_write(0xa000, 0);
    assert_eq!(m.cpu_read(0x8100), Some(0));
    assert_eq!(m.cpu_read(0xa100), Some(1), "one 16 KB window, not MMC2's 8 KB");
    assert_eq!(m.cpu_read(0xc100), Some(6));
    m.cpu_write(0xa000, 1);
    assert_eq!(m.cpu_read(0x8100), Some(2));

    m.cpu_write(0x6001, 0x5a);
    assert_eq!(m.cpu_read(0x6001), Some(0x5a));
    assert_eq!(m.battery_ram().map(|r| r[1]), Some(0x5a));

    m.cpu_write(0xf000, 1);
    assert_eq!(m.mirroring(), Mirroring::Horizontal);
    m.cpu_write(0xf000, 0);
    assert_eq!(m.mirroring(), Mirroring::Vertical);
}

#[test]
fn mmc2_without_chr_rom_is_mislabelled_rather_than_unusual() {
    // The whole point of the board is latching between CHR banks, so an image
    // with none is a wrong header, not a variant to support.
    assert!(Cartridge::load(&rom(9, 0)).is_err());
}

#[test]
fn fme7_maps_prg_through_a_command_and_parameter_pair() {
    let mut c = Cartridge::load(&rom(69, 4)).unwrap();
    let m = c.mapper.as_mut();
    // Command 9 is the $8000 window; command $B is $C000.
    m.cpu_write(0x8000, 9);
    m.cpu_write(0xa000, 2);
    assert_eq!(m.cpu_read(0x8100), Some(2));
    m.cpu_write(0x8000, 0x0b);
    m.cpu_write(0xa000, 0);
    assert_eq!(m.cpu_read(0xc100), Some(0));
    // $E000 is hard-wired to the last bank.
    assert_eq!(m.cpu_read(0xe100), Some(7));

    // Command 0-7 are 1 KB CHR banks.
    m.cpu_write(0x8000, 0);
    m.cpu_write(0xa000, 5);
    assert_eq!(m.ppu_read(0), 5);
    m.cpu_write(0x8000, 7);
    m.cpu_write(0xa000, 9);
    assert_eq!(m.ppu_read(0x1c00), 9);

    // Command $C is mirroring, and this board can do all four arrangements.
    for (value, expected) in [
        (0, Mirroring::Vertical),
        (1, Mirroring::Horizontal),
        (2, Mirroring::SingleScreenLo),
        (3, Mirroring::SingleScreenHi),
    ] {
        m.cpu_write(0x8000, 0x0c);
        m.cpu_write(0xa000, value);
        assert_eq!(m.mirroring(), expected);
    }
}

#[test]
fn fme7_can_put_either_rom_or_ram_at_6000() {
    let mut c = Cartridge::load(&rom(69, 4)).unwrap();
    let m = c.mapper.as_mut();
    // Bit 7 clear: the window is a PRG ROM bank.
    m.cpu_write(0x8000, 8);
    m.cpu_write(0xa000, 3);
    assert_eq!(m.cpu_read(0x6100), Some(3));

    // Bits 7 and 6 set: RAM, and answering.
    m.cpu_write(0xa000, 0xc0);
    m.cpu_write(0x6100, 0x42);
    assert_eq!(m.cpu_read(0x6100), Some(0x42));

    // Bit 7 set but bit 6 clear: RAM selected and disabled, so the bus is open
    // rather than reading back a zero that the game would believe.
    m.cpu_write(0xa000, 0x80);
    assert_eq!(m.cpu_read(0x6100), None);
    m.cpu_write(0x6100, 0x99);
    m.cpu_write(0xa000, 0xc0);
    assert_eq!(m.cpu_read(0x6100), Some(0x42), "a disabled write must not land");
}

#[test]
fn fme7_counts_cpu_cycles_down_to_an_interrupt() {
    let mut c = Cartridge::load(&rom(69, 4)).unwrap();
    let m = c.mapper.as_mut();
    // A three-cycle count, then enable both the counter and the interrupt.
    m.cpu_write(0x8000, 0x0e);
    m.cpu_write(0xa000, 3);
    m.cpu_write(0x8000, 0x0f);
    m.cpu_write(0xa000, 0);
    m.cpu_write(0x8000, 0x0d);
    m.cpu_write(0xa000, 0x81);

    for cycle in 0..3 {
        m.tick();
        assert!(!m.irq(), "fired early, at cycle {cycle}");
    }
    // The interrupt is on the wrap past zero, not on reaching it.
    m.tick();
    assert!(m.irq());

    // Writing the control register is how the handler acknowledges it.
    m.cpu_write(0x8000, 0x0d);
    m.cpu_write(0xa000, 0x81);
    assert!(!m.irq());

    // With the counter switched off it stops dead rather than wrapping around.
    m.cpu_write(0x8000, 0x0d);
    m.cpu_write(0xa000, 0x00);
    for _ in 0..70_000 {
        m.tick();
    }
    assert!(!m.irq());
}

#[test]
fn every_tier_two_board_survives_a_savestate_round_trip() {
    for mapper in [9u16, 10, 11, 69, 71] {
        let mut c = Cartridge::load(&rom(mapper, 4)).unwrap();
        let m = c.mapper.as_mut();
        // Move something on each board, so the state under test is not the
        // power-on one.
        match mapper {
            9 | 10 => {
                m.cpu_write(0xa000, 2);
                m.cpu_write(0xb000, 1);
                m.ppu_read(0x0fd8);
            }
            11 | 71 => m.cpu_write(0xc000, 1),
            _ => {
                m.cpu_write(0x8000, 9);
                m.cpu_write(0xa000, 2);
            }
        }
        let mut saved = Vec::new();
        m.save_state(&mut saved);
        let before = (0x8100u16..0x8110).map(|a| m.cpu_read(a)).collect::<Vec<_>>();
        let chr_before = (0u16..16).map(|a| m.ppu_read(a)).collect::<Vec<_>>();

        // A fresh board, then the state put back into it.
        let mut fresh = Cartridge::load(&rom(mapper, 4)).unwrap();
        let f = fresh.mapper.as_mut();
        let mut input = saved.as_slice();
        f.load_state(&mut input).unwrap();
        assert!(input.is_empty(), "mapper {mapper} left {} bytes unread", input.len());
        assert_eq!(before, (0x8100u16..0x8110).map(|a| f.cpu_read(a)).collect::<Vec<_>>());
        assert_eq!(chr_before, (0u16..16).map(|a| f.ppu_read(a)).collect::<Vec<_>>());
    }
}
