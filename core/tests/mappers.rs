mod common;
use nes_core::cart::{mapper::Mapper, Cartridge, Mirroring};
fn serial(m: &mut dyn Mapper, addr: u16, value: u8) {
    for bit in 0..5 {
        m.tick();
        m.tick();
        m.cpu_write(addr, (value >> bit) & 1);
    }
}
#[test]
fn uxrom_cnrom_axrom_banks() {
    for kind in [2, 3, 7] {
        let mut c = Cartridge::load(&common::rom(kind, 4)).unwrap();
        let m = c.mapper.as_mut();
        m.cpu_write(0x8000, 1);
        match kind {
            2 => {
                assert_eq!(m.cpu_read(0x8100), Some(2));
                assert_eq!(m.cpu_read(0xc100), Some(6));
            }
            3 => {
                assert_eq!(m.ppu_read(0), 8);
                assert_eq!(m.ppu_read(0x1fff), 15);
            }
            7 => {
                assert_eq!(m.cpu_read(0x8100), Some(4));
                m.cpu_write(0x8000, 0x10);
                assert_eq!(m.mirroring(), Mirroring::SingleScreenHi);
            }
            _ => unreachable!(),
        }
    }
}
#[test]
fn mmc1_serial_modes_chr_and_ram_gate() {
    let mut c = Cartridge::load(&common::rom(1, 4)).unwrap();
    let m = c.mapper.as_mut();
    serial(m, 0xe000, 2);
    assert_eq!(m.cpu_read(0x8100), Some(4));
    assert_eq!(m.cpu_read(0xc100), Some(6));
    serial(m, 0x8000, 0x1a);
    serial(m, 0xa000, 3);
    serial(m, 0xc000, 5);
    assert_eq!(m.mirroring(), Mirroring::Vertical);
    assert_eq!(m.cpu_read(0x8100), Some(0));
    assert_eq!(m.cpu_read(0xc100), Some(4));
    assert_eq!(m.ppu_read(0), 12);
    assert_eq!(m.ppu_read(0x1000), 20);
    serial(m, 0xe000, 0x10);
    assert_eq!(m.cpu_read(0x6000), None);
}
#[test]
fn mmc3_banks_ram_protection_and_filtered_irq() {
    let mut c = Cartridge::load(&common::rom(4, 4)).unwrap();
    let m = c.mapper.as_mut();
    m.cpu_write(0x8000, 6);
    m.cpu_write(0x8001, 3);
    assert_eq!(m.cpu_read(0x8100), Some(3));
    assert_eq!(m.cpu_read(0xc100), Some(6));
    m.cpu_write(0x8000, 0x46);
    assert_eq!(m.cpu_read(0x8100), Some(6));
    assert_eq!(m.cpu_read(0xc100), Some(3));
    m.cpu_write(0x8000, 0);
    m.cpu_write(0x8001, 7);
    assert_eq!(m.ppu_read(0), 6);
    assert_eq!(m.ppu_read(0x400), 7);
    m.cpu_write(0x6000, 42);
    m.cpu_write(0xa001, 0xc0);
    m.cpu_write(0x6000, 99);
    assert_eq!(m.cpu_read(0x6000), Some(42));
    m.cpu_write(0xc000, 1);
    m.cpu_write(0xc001, 0);
    m.cpu_write(0xe001, 0);
    m.ppu_bus(0, 0);
    m.ppu_bus(0x1000, 10);
    assert!(!m.irq());
    m.ppu_bus(0, 11);
    m.ppu_bus(0x1000, 14);
    assert!(!m.irq()); // rejected short pulse
    m.ppu_bus(0, 15);
    m.ppu_bus(0x1000, 25);
    assert!(m.irq());
    m.cpu_write(0xe000, 0);
    assert!(!m.irq());
}
/// GxROM: 32 KB PRG and 8 KB CHR banks from one register, with bus conflicts.
/// Every 8 KB unit carries its own index, and `0xff` at offset `0x100` so a write
/// there passes the value through unmasked.
fn gxrom(prg_16k: usize, chr_8k: usize) -> Vec<u8> {
    let mut rom = vec![0; 16 + prg_16k * 16384 + chr_8k * 8192];
    rom[..4].copy_from_slice(b"NES\x1a");
    rom[4] = prg_16k as u8;
    rom[5] = chr_8k as u8;
    // Mapper 66 splits as $4 in flags7 and $2 in flags6.
    rom[6] = 0x20;
    rom[7] = 0x40;
    for unit in 0..prg_16k * 2 {
        let start = 16 + unit * 8192;
        rom[start..start + 8192].fill(unit as u8);
        rom[start + 0x100] = 0xff;
    }
    for bank in 0..chr_8k {
        let start = 16 + prg_16k * 16384 + bank * 8192;
        rom[start..start + 8192].fill(0xc0 + bank as u8);
    }
    rom
}
#[test]
fn gxrom_selects_prg_and_chr_banks_through_bus_conflicts() {
    let mut c = Cartridge::load(&gxrom(8, 4)).unwrap();
    assert_eq!(c.header.mapper, 66);
    let m = c.mapper.as_mut();
    // Reset state selects the first 32 KB PRG bank and the first CHR bank.
    assert_eq!(m.cpu_read(0x8000), Some(0));
    assert_eq!(m.cpu_read(0xffff), Some(3));
    assert_eq!(m.ppu_read(0), 0xc0);

    // $ff at $8100 leaves the written value intact: PRG bank 2, CHR bank 1.
    m.cpu_write(0x8100, 0x21);
    assert_eq!(m.cpu_read(0x8000), Some(8));
    assert_eq!(m.cpu_read(0xc000), Some(10));
    assert_eq!(m.ppu_read(0), 0xc1);

    // $8200 now reads 8, so the write is ANDed down to $08 - PRG 0, CHR 0.
    m.cpu_write(0x8200, 0x3f);
    assert_eq!(m.cpu_read(0x8000), Some(0));
    assert_eq!(m.ppu_read(0x1fff), 0xc0);

    // Only bits 4-5 and 0-1 are decoded, and mirroring stays as the header set it.
    m.cpu_write(0x8100, 0xff);
    assert_eq!(m.cpu_read(0x8000), Some(12));
    assert_eq!(m.ppu_read(0), 0xc3);
    assert_eq!(m.mirroring(), Mirroring::Horizontal);
}
