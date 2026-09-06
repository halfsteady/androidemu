//! Header parsing and the NROM mapper, driven by synthetic ROM images.

use nes_core::cart::{Cartridge, Mirroring, Region};

/// Build an iNES image. `prg_banks` counts 16 KB units, `chr_banks` 8 KB units.
fn ines(prg_banks: u8, chr_banks: u8, flags6: u8, flags7: u8) -> Vec<u8> {
    let mut v = vec![0u8; 16];
    v[0..4].copy_from_slice(b"NES\x1A");
    v[4] = prg_banks;
    v[5] = chr_banks;
    v[6] = flags6;
    v[7] = flags7;
    v.resize(
        16 + prg_banks as usize * 16384 + chr_banks as usize * 8192,
        0,
    );
    v
}

#[test]
fn rejects_files_that_are_not_ines() {
    assert!(Cartridge::load(b"not a rom at all").is_err());
    assert!(Cartridge::load(&[]).is_err());
}

#[test]
fn parses_mapper_number_from_both_nibbles() {
    // Mapper 0 in the low nibble of flags6, 0 in the high nibble of flags7.
    let rom = ines(1, 1, 0x00, 0x00);
    assert_eq!(Cartridge::load(&rom).unwrap().header.mapper, 0);

    // Mapper 5 would be flags6 = $50; it is not implemented yet, so this asserts
    // the number is decoded rather than that the board works.
    let rom = ines(1, 1, 0x50, 0x00);
    match Cartridge::load(&rom) {
        Err(nes_core::cart::CartError::UnsupportedMapper(n)) => assert_eq!(n, 5),
        other => panic!("expected UnsupportedMapper(5), got {other:?}"),
    }
}

#[test]
fn reads_mirroring_and_battery_from_flags6() {
    let rom = ines(1, 1, 0x00, 0x00);
    assert_eq!(
        Cartridge::load(&rom).unwrap().header.mirroring,
        Mirroring::Horizontal
    );

    let rom = ines(1, 1, 0x01, 0x00);
    assert_eq!(
        Cartridge::load(&rom).unwrap().header.mirroring,
        Mirroring::Vertical
    );

    let rom = ines(1, 1, 0x08, 0x00);
    assert_eq!(
        Cartridge::load(&rom).unwrap().header.mirroring,
        Mirroring::FourScreen
    );

    let rom = ines(1, 1, 0x02, 0x00);
    assert!(Cartridge::load(&rom).unwrap().header.battery);
}

#[test]
fn detects_nes2_and_decodes_its_extra_fields() {
    let mut rom = ines(1, 1, 0x00, 0x08); // flags7 bits 3-2 = 10 marks NES 2.0
    rom[8] = 0x01; // mapper high nibble 0 -> mapper stays low, submapper 0
    rom[12] = 0x01; // PAL
    let cart = Cartridge::load(&rom);
    // Mapper 256 is not implemented; what matters is that the field was decoded.
    match cart {
        Err(nes_core::cart::CartError::UnsupportedMapper(n)) => {
            assert_eq!(
                n, 256,
                "the mapper high nibble from byte 8 should be applied"
            )
        }
        other => panic!("expected UnsupportedMapper(256), got {other:?}"),
    }

    let mut rom = ines(1, 1, 0x00, 0x08);
    rom[8] = 0x00;
    rom[12] = 0x01;
    let cart = Cartridge::load(&rom).unwrap();
    assert!(cart.header.nes2);
    assert_eq!(cart.header.region, Region::Pal);
}

#[test]
fn a_16k_prg_image_mirrors_into_both_halves() {
    // This is why a 16 KB game's reset vector at $FFFC reads correctly.
    let mut rom = ines(1, 1, 0x00, 0x00);
    rom[16] = 0xAA; // first byte of PRG, seen at both $8000 and $C000
    rom[16 + 0x3FFC] = 0x00;
    rom[16 + 0x3FFD] = 0xC0;
    let mut cart = Cartridge::load(&rom).unwrap();
    assert_eq!(cart.mapper.cpu_read(0x8000), Some(0xAA));
    assert_eq!(
        cart.mapper.cpu_read(0xC000),
        Some(0xAA),
        "16 KB PRG must mirror"
    );
    assert_eq!(cart.mapper.cpu_read(0xFFFC), Some(0x00));
    assert_eq!(cart.mapper.cpu_read(0xFFFD), Some(0xC0));
}

#[test]
fn a_32k_prg_image_does_not_mirror() {
    let mut rom = ines(2, 1, 0x00, 0x00);
    rom[16] = 0xAA;
    rom[16 + 0x4000] = 0xBB;
    let mut cart = Cartridge::load(&rom).unwrap();
    assert_eq!(cart.mapper.cpu_read(0x8000), Some(0xAA));
    assert_eq!(cart.mapper.cpu_read(0xC000), Some(0xBB));
}

#[test]
fn chr_ram_is_writable_and_chr_rom_is_not() {
    // chr_banks = 0 means the board carries CHR RAM.
    let rom = ines(1, 0, 0x00, 0x00);
    let mut cart = Cartridge::load(&rom).unwrap();
    cart.mapper.ppu_write(0x0000, 0x5A);
    assert_eq!(
        cart.mapper.ppu_read(0x0000),
        0x5A,
        "CHR RAM should accept writes"
    );

    let rom = ines(1, 1, 0x00, 0x00);
    let mut cart = Cartridge::load(&rom).unwrap();
    cart.mapper.ppu_write(0x0000, 0x5A);
    assert_eq!(
        cart.mapper.ppu_read(0x0000),
        0x00,
        "CHR ROM should ignore writes"
    );
}

#[test]
fn prg_ram_at_6000_reads_back_what_was_written() {
    let rom = ines(1, 1, 0x00, 0x00);
    let mut cart = Cartridge::load(&rom).unwrap();
    cart.mapper.cpu_write(0x6123, 0x77);
    assert_eq!(cart.mapper.cpu_read(0x6123), Some(0x77));
}

#[test]
fn rom_hash_ignores_the_header() {
    // Two images with identical payloads but different headers must hash the same,
    // so a bad header can be corrected by looking the ROM up.
    let a = ines(1, 1, 0x00, 0x00);
    let b = ines(1, 1, 0x01, 0x00); // different mirroring bit
    let ha = Cartridge::load(&a).unwrap().header.hash;
    let hb = Cartridge::load(&b).unwrap().header.hash;
    assert_eq!(ha, hb, "the hash must identify the payload, not the header");
}

#[test]
fn rejects_truncated_chr_empty_prg_and_overflowing_exponents() {
    let mut rom = ines(1, 1, 0, 0);
    rom.pop();
    assert!(Cartridge::load(&rom).is_err());
    assert!(Cartridge::load(&ines(0, 1, 0, 0)).is_err());
    let mut rom = ines(1, 0, 0, 8);
    rom[4] = 0xff;
    rom[9] = 0x0f;
    assert!(Cartridge::load(&rom).is_err());
}
