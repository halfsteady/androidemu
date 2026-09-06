pub fn rom(mapper: u8, chr: u8) -> Vec<u8> {
    let mut rom = vec![0; 16 + 4 * 16384 + chr as usize * 8192];
    rom[..4].copy_from_slice(b"NES\x1a");
    rom[4] = 4;
    rom[5] = chr;
    rom[6] = (mapper << 4) | 2;
    for bank in 0..8 {
        rom[16 + bank * 8192..16 + (bank + 1) * 8192].fill(bank as u8);
    }
    for bank in 0..chr as usize * 8 {
        rom[16 + 65536 + bank * 1024..16 + 65536 + (bank + 1) * 1024].fill(bank as u8);
    }
    // Every PRG bank has a stable JMP $8000 loop and reset vector.
    for bank in 0..4 {
        let start = 16 + bank * 16384;
        rom[start..start + 3].copy_from_slice(&[0x4c, 0, 0x80]);
        rom[start + 0x3ffc..start + 0x4000].copy_from_slice(&[0, 0x80, 0, 0x80]);
    }
    rom
}
