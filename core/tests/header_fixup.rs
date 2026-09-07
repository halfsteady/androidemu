//! Header repair, driven by the mangled headers that actually turn up in the wild.
//!
//! These are the fixes the file itself proves, so they need no database and can
//! be checked exactly. The override table is data and ships empty; what is tested
//! here is the mechanism that applies it and the rules that need nobody's help.

use nes_core::cart::fixup::Fixes;
use nes_core::cart::{Cartridge, Mirroring, Region};

/// Build an iNES image. `prg_banks` counts 16 KB units, `chr_banks` 8 KB units.
fn ines(prg_banks: u8, chr_banks: u8, flags6: u8, flags7: u8) -> Vec<u8> {
    let mut v = vec![0u8; 16];
    v[0..4].copy_from_slice(b"NES\x1A");
    v[4] = prg_banks;
    v[5] = chr_banks;
    v[6] = flags6;
    v[7] = flags7;
    v.resize(16 + prg_banks as usize * 16384 + chr_banks as usize * 8192, 0);
    v
}

#[test]
fn a_clean_header_is_left_completely_alone() {
    let cart = Cartridge::load(&ines(2, 1, 0x01, 0x00)).unwrap();
    assert!(cart.header.fixes.is_empty(), "{:?}", cart.header.fixes);
    assert_eq!(cart.header.mapper, 0);
    assert_eq!(cart.header.mirroring, Mirroring::Vertical);
    assert_eq!(0, cart.header.fixes.labels().count());
}

#[test]
fn disk_dude_stops_the_high_mapper_bits_being_believed() {
    // The calling card of a long-dead ripping tool, written straight over bytes
    // 7-15. Byte 7 becomes 'D' = 0x44, and since byte 7 carries mapper bits 4-7
    // a plain NROM game ends up claiming mapper 64 and failing to load at all.
    let mut rom = ines(2, 1, 0x00, 0x00);
    rom[7..16].copy_from_slice(b"DiskDude!");
    let clean = Cartridge::load(&ines(2, 1, 0x00, 0x00)).unwrap();

    let cart = Cartridge::load(&rom).unwrap();
    assert_eq!(cart.header.mapper, 0, "the high mapper bits must be discarded");
    assert!(cart.header.fixes.has(Fixes::ARCHAIC_MAPPER));
    assert!(cart.header.fixes.has(Fixes::DISK_DUDE));
    // Byte 9 is 'k', whose bit 0 is the iNES PAL flag. Left believed, an NTSC
    // game would run its whole soundtrack at 50 Hz.
    assert!(cart.header.fixes.has(Fixes::ARCHAIC_REGION));
    assert_eq!(cart.header.region, Region::Ntsc);
    // Same ROM either way, so the identity that keys the override table has to
    // agree even though the two headers do not.
    assert_eq!(clean.header.identity, cart.header.identity);
}

#[test]
fn junk_in_the_tail_bytes_discards_the_high_mapper_bits() {
    // Mapper 4 in flags6's high nibble, plus 16 from flags7's: mapper 20, which
    // is not implemented. Junk in bytes 12-15 says flags7 is not to be believed,
    // and discarding it leaves MMC3 — a refusal to load becomes a game.
    let mut rom = ines(2, 1, 0x40, 0x10);
    rom[13] = 0x77;
    let cart = Cartridge::load(&rom).unwrap();
    assert_eq!(cart.header.mapper, 4);
    assert!(cart.header.fixes.has(Fixes::ARCHAIC_MAPPER));
    assert!(!cart.header.fixes.has(Fixes::DISK_DUDE));

    // With the tail clean, flags7 is believed and mapper 20 stands — unsupported,
    // which is the honest answer rather than a silently different board.
    match Cartridge::load(&ines(2, 1, 0x40, 0x10)) {
        Err(nes_core::cart::CartError::UnsupportedMapper(n)) => assert_eq!(n, 20),
        other => panic!("expected UnsupportedMapper(20), got {other:?}"),
    }
}

#[test]
fn a_header_claiming_chr_a_chr_ram_game_does_not_have_is_corrected() {
    // The common lie: the board has CHR RAM, the header claims one CHR bank, and
    // the file simply stops after the program. Trimming to what is there is the
    // difference between loading and refusing.
    let mut rom = ines(2, 1, 0x00, 0x00);
    rom.truncate(16 + 2 * 16384);
    let cart = Cartridge::load(&rom).unwrap();
    assert_eq!(cart.header.chr_rom_size, 0);
    assert_eq!(cart.header.chr_ram_size, 8 * 1024, "CHR RAM has to be provided");
    assert!(cart.header.fixes.has(Fixes::CHR_TRUNCATED));
}

#[test]
fn a_whole_missing_chr_bank_is_trimmed_but_a_partial_one_is_damage() {
    // Two banks claimed, one present: the header overstated the board.
    let mut rom = ines(2, 2, 0x00, 0x00);
    rom.truncate(16 + 2 * 16384 + 8192);
    let cart = Cartridge::load(&rom).unwrap();
    assert_eq!(cart.header.chr_rom_size, 8192);
    assert!(cart.header.fixes.has(Fixes::CHR_TRUNCATED));

    // One byte short of a bank is a damaged file, not a mislabelled one. Trimming
    // would throw away real graphics and hand back a game that renders garbage,
    // so this stays a refusal.
    let mut rom = ines(2, 2, 0x00, 0x00);
    rom.truncate(16 + 2 * 16384 + 8192 - 1);
    assert!(Cartridge::load(&rom).is_err());
}

#[test]
fn a_whole_missing_prg_bank_is_trimmed_but_a_partial_one_is_damage() {
    let mut rom = ines(4, 0, 0x00, 0x00);
    rom.truncate(16 + 2 * 16384);
    let cart = Cartridge::load(&rom).unwrap();
    assert_eq!(cart.header.prg_rom_size, 2 * 16384);
    assert!(cart.header.fixes.has(Fixes::PRG_TRUNCATED));

    let mut rom = ines(4, 0, 0x00, 0x00);
    rom.truncate(16 + 2 * 16384 - 1);
    assert!(Cartridge::load(&rom).is_err());
}

#[test]
fn nrom_that_cannot_reach_its_own_program_is_reported() {
    // Mapper 0 has no banking hardware, so 64 KB of PRG is impossible. Which
    // mapper it should be is per-game knowledge, so this is reported and not
    // guessed at — the guess is what the override table is for.
    let rom = ines(4, 1, 0x00, 0x00);
    let cart = Cartridge::load(&rom).unwrap();
    assert_eq!(cart.header.mapper, 0);
    assert!(cart.header.fixes.has(Fixes::IMPLAUSIBLE_MAPPER));
    assert!(!cart.header.fixes.from_table());
}

#[test]
fn the_identity_ignores_the_header_but_the_payload_hash_does_not() {
    let plain = Cartridge::load(&ines(2, 1, 0x00, 0x00)).unwrap().header;
    // Same payload, a header that lies about mirroring and battery.
    let lying = Cartridge::load(&ines(2, 1, 0x0B, 0x00)).unwrap().header;
    assert_eq!(plain.identity, lying.identity, "identity keys the override table");
    assert_eq!(plain.hash, lying.hash, "both describe the same payload extent");

    // A trimmed CHR describes a different payload, so the savestate hash moves
    // with it — which is right, because the machine is configured differently.
    let mut short = ines(2, 1, 0x00, 0x00);
    short.truncate(16 + 2 * 16384);
    let trimmed = Cartridge::load(&short).unwrap().header;
    assert_ne!(plain.hash, trimmed.hash);
}

#[test]
fn nes2_headers_are_trusted_and_never_treated_as_archaic() {
    // NES 2.0 uses bytes 8-15 deliberately, so the archaic rule must not fire on
    // them and discard a legitimate high mapper nibble.
    let mut rom = ines(2, 1, 0x40, 0x08);
    rom[12] = 0x01; // a real NES 2.0 region byte: PAL
    let cart = Cartridge::load(&rom).unwrap();
    assert!(cart.header.nes2);
    assert_eq!(cart.header.mapper, 4);
    assert!(cart.header.fixes.is_empty(), "{:?}", cart.header.fixes);
    assert_eq!(cart.header.region, Region::Pal, "a real PAL flag must survive");
}

#[test]
fn the_shipped_override_table_changes_nothing() {
    // It is empty on purpose: a row nobody has checked against a real dump would
    // silently mis-configure a real game. This asserts the mechanism is wired in
    // and inert, so a populated table is the only thing that can change a header.
    let cart = Cartridge::load(&ines(2, 1, 0x00, 0x00)).unwrap();
    assert!(!cart.header.fixes.from_table());
    assert!(nes_core::cart::fixup::lookup(cart.header.identity).is_none());
}
