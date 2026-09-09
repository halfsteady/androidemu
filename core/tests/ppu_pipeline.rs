//! Cycle-level PPU pipeline behavior: the OAM data bus during sprite
//! evaluation and fetch, the frozen secondary OAM address, misaligned OAM,
//! the two-dot address bus, PPUDATA timing and OAM row corruption.
//!
//! The dot-by-dot OAM bus traces come from hardware captures documented in
//! 100th Coin's AccuracyCoin test ROM (MIT), "$2004 Stress Test".
use nes_core::{cart::Cartridge, cpu::Bus, ppu::Ppu, NesBus};

mod common;

fn cart() -> Cartridge {
    // No CHR banks: 8 KB of CHR RAM the tests can draw into.
    Cartridge::load(&common::rom(0, 0)).unwrap()
}
fn bus() -> NesBus {
    NesBus::new(cart())
}
fn to(p: &mut Ppu, c: &mut Cartridge, line: u16, dot: u16) {
    for _ in 0..341 * 262 * 2 {
        if p.scanline == line && p.dot == dot {
            return;
        }
        p.tick(c.mapper.as_mut());
    }
    panic!("target dot not reached");
}
fn ticks(p: &mut Ppu, c: &mut Cartridge, n: usize) {
    for _ in 0..n {
        p.tick(c.mapper.as_mut());
    }
}
/// A CPU access followed by the rest of its instruction: registers settle
/// before the next access, as they would with real instruction spacing.
fn write(p: &mut Ppu, c: &mut Cartridge, addr: u16, value: u8) {
    p.write_register(addr, value, c.mapper.as_mut());
    ticks(p, c, 12);
}
fn set_v(p: &mut Ppu, c: &mut Cartridge, addr: u16) {
    write(p, c, 0x2006, (addr >> 8) as u8);
    write(p, c, 0x2006, addr as u8);
}
fn chr(c: &mut Cartridge, addr: u16, value: u8) {
    c.mapper.ppu_write(addr, value);
}

/// OAM data bus, dot by dot on one scanline, with fewer than eight sprites
/// in range (OAM holds $FF down to $00).
const OAM_BUS_FEWER_THAN_EIGHT: [u8; 341] = [
    0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xfb, 0xfb, 0xf7, 0xf7, 0xf3, 0xf3, 0xef, 0xef, 0xeb, 0xeb, 0xe7, 0xe7, 0xe3,
    0xe3, 0xdf, 0xdf, 0xdb, 0xdb, 0xd7, 0xd7, 0xd3, 0xd3, 0xcf, 0xcf, 0xcb, 0xcb, 0xc7, 0xc7, 0xc3,
    0xc3, 0xbf, 0xbf, 0xbb, 0xbb, 0xb7, 0xb7, 0xb3, 0xb3, 0xaf, 0xaf, 0xab, 0xab, 0xa7, 0xa7, 0xa3,
    0xa3, 0x9f, 0x9f, 0x9b, 0x9b, 0x97, 0x97, 0x93, 0x93, 0x8f, 0x8f, 0x8b, 0x8b, 0x87, 0x87, 0x83,
    0x83, 0x7f, 0x7f, 0x7e, 0x7e, 0x61, 0x61, 0x7c, 0x7c, 0x7b, 0x7b, 0x7a, 0x7a, 0x61, 0x61, 0x78,
    0x78, 0x77, 0x77, 0x73, 0x73, 0x6f, 0x6f, 0x6b, 0x6b, 0x67, 0x67, 0x63, 0x63, 0x5f, 0x5f, 0x5b,
    0x5b, 0x57, 0x57, 0x53, 0x53, 0x4f, 0x4f, 0x4b, 0x4b, 0x47, 0x47, 0x43, 0x43, 0x3f, 0x3f, 0x3b,
    0x3b, 0x37, 0x37, 0x33, 0x33, 0x2f, 0x2f, 0x2b, 0x2b, 0x27, 0x27, 0x23, 0x23, 0x1f, 0x1f, 0x1b,
    0x1b, 0x17, 0x17, 0x13, 0x13, 0x0f, 0x0f, 0x0b, 0x0b, 0x07, 0x07, 0x03, 0x03, 0xff, 0x03, 0xfb,
    0x03, 0xf7, 0x03, 0xf3, 0x03, 0xef, 0x03, 0xeb, 0x03, 0xe7, 0x03, 0xe3, 0x03, 0xdf, 0x03, 0xdb,
    0x03, 0xd7, 0x03, 0xd3, 0x03, 0xcf, 0x03, 0xcb, 0x03, 0xc7, 0x03, 0xc3, 0x03, 0xbf, 0x03, 0xbb,
    0x03, 0xb7, 0x03, 0xb3, 0x03, 0xaf, 0x03, 0xab, 0x03, 0xa7, 0x03, 0xa3, 0x03, 0x9f, 0x03, 0x9b,
    0x03, 0x7f, 0x7e, 0x61, 0x7c, 0x7c, 0x7c, 0x7c, 0x7c, 0x7b, 0x7a, 0x61, 0x78, 0x78, 0x78, 0x78,
    0x78, 0x03, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f,
    0x7f, 0x7f, 0x7f, 0x7f, 0x7f,
];
/// The same with eight in-range sprites followed by the overflow scan.
const OAM_BUS_EIGHT_AND_OVERFLOW: [u8; 341] = [
    0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0x80, 0x80, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0x7f, 0x7f, 0x01, 0x01, 0x20, 0x20, 0xee,
    0xee, 0x7e, 0x7e, 0x02, 0x02, 0x40, 0x40, 0xdd, 0xdd, 0x7d, 0x7d, 0x03, 0x03, 0x60, 0x60, 0xcc,
    0xcc, 0x7c, 0x7c, 0x04, 0x04, 0x80, 0x80, 0xbb, 0xbb, 0x7b, 0x7b, 0x05, 0x05, 0xa0, 0xa0, 0xaa,
    0xaa, 0x7a, 0x7a, 0x06, 0x06, 0xc0, 0xc0, 0x99, 0x99, 0x79, 0x79, 0x07, 0x07, 0xe0, 0xe0, 0x88,
    0x88, 0x00, 0x80, 0x05, 0x80, 0x02, 0x80, 0x0f, 0x80, 0x10, 0x80, 0x15, 0x80, 0x02, 0x80, 0x1f,
    0x80, 0x20, 0x80, 0x25, 0x80, 0x22, 0x80, 0x2f, 0x80, 0x30, 0x80, 0x35, 0x80, 0x22, 0x80, 0x3f,
    0x80, 0x40, 0x80, 0x45, 0x80, 0x42, 0x80, 0x4f, 0x80, 0x50, 0x80, 0x55, 0x80, 0x42, 0x80, 0x5f,
    0x80, 0x60, 0x80, 0x65, 0x80, 0x62, 0x80, 0x6f, 0x80, 0x70, 0x80, 0x75, 0x80, 0x62, 0x80, 0x7f,
    0x80, 0x80, 0x80, 0x81, 0x80, 0x82, 0x80, 0x80, 0x80, 0x84, 0x80, 0x88, 0x80, 0x8c, 0x80, 0x90,
    0x80, 0x94, 0x80, 0x98, 0x80, 0x9c, 0x80, 0xa0, 0x80, 0xa4, 0x80, 0xa8, 0x80, 0xac, 0x80, 0xb0,
    0x80, 0xb4, 0x80, 0xb8, 0x80, 0xbc, 0x80, 0xc0, 0x80, 0xc4, 0x80, 0xc8, 0x80, 0xcc, 0x80, 0xd0,
    0x80, 0xd4, 0x80, 0xd8, 0x80, 0xdc, 0x80, 0x80, 0x80, 0x7f, 0x80, 0x7e, 0x80, 0x7d, 0x80, 0x7c,
    0x80, 0x80, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f, 0x01, 0x20, 0xee, 0xee, 0xee, 0xee,
    0xee, 0x7e, 0x02, 0x40, 0xdd, 0xdd, 0xdd, 0xdd, 0xdd, 0x7d, 0x03, 0x60, 0xcc, 0xcc, 0xcc, 0xcc,
    0xcc, 0x7c, 0x04, 0x80, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0x7b, 0x05, 0xa0, 0xaa, 0xaa, 0xaa, 0xaa,
    0xaa, 0x7a, 0x06, 0xc0, 0x99, 0x99, 0x99, 0x99, 0x99, 0x79, 0x07, 0xe0, 0x88, 0x88, 0x88, 0x88,
    0x88, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
    0x80, 0x80, 0x80, 0x80, 0x80,
];
/// The first 32 OAM bytes for the eight-sprite trace; the rest is $00..$BF.
const EIGHT_SPRITE_OAM: [u8; 32] = [
    0x80, 0x00, 0x00, 0xff, 0x7f, 0x01, 0x20, 0xee, 0x7e, 0x02, 0x40, 0xdd, 0x7d, 0x03, 0x60, 0xcc,
    0x7c, 0x04, 0x80, 0xbb, 0x7b, 0x05, 0xa0, 0xaa, 0x7a, 0x06, 0xc0, 0x99, 0x79, 0x07, 0xe0, 0x88,
];

/// Read $2004 with the PPU parked at every dot of scanline 128 and compare
/// the OAM data bus with the hardware trace.
fn oam_bus_trace(oam: &[u8; 256]) -> Vec<u8> {
    let mut c = cart();
    let mut p = Ppu::new();
    p.oam = *oam;
    p.mask = 0x18;
    to(&mut p, &mut c, 127, 0);
    let mut trace = Vec::new();
    for dot in 0..341 {
        to(&mut p, &mut c, 128, dot);
        let mut probe = p.clone();
        trace.push(probe.read_register(0x2004, c.mapper.as_mut()));
    }
    trace
}

#[test]
fn oam_data_bus_with_fewer_than_eight_sprites_in_range() {
    let oam: [u8; 256] = std::array::from_fn(|i| 0xff - i as u8);
    let trace = oam_bus_trace(&oam);
    let bad: Vec<String> = (0..341)
        .filter(|&d| trace[d] != OAM_BUS_FEWER_THAN_EIGHT[d])
        .map(|d| {
            format!(
                "dot {d}: got {:02x} want {:02x}",
                trace[d], OAM_BUS_FEWER_THAN_EIGHT[d]
            )
        })
        .collect();
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn oam_data_bus_with_eight_sprites_and_the_overflow_scan() {
    let mut oam = [0u8; 256];
    oam[..32].copy_from_slice(&EIGHT_SPRITE_OAM);
    for (i, byte) in oam[32..].iter_mut().enumerate() {
        *byte = i as u8;
    }
    let trace = oam_bus_trace(&oam);
    let bad: Vec<String> = (0..341)
        .filter(|&d| trace[d] != OAM_BUS_EIGHT_AND_OVERFLOW[d])
        .map(|d| {
            format!(
                "dot {d}: got {:02x} want {:02x}",
                trace[d], OAM_BUS_EIGHT_AND_OVERFLOW[d]
            )
        })
        .collect();
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// Nine in-range sprites set the overflow flag; misaligned OAM addresses walk
/// through them with the +1 / +4&$FC / +5 rules and the X-position check.
fn overflow_after_misaligned_start(oam_addr: u8, layout: &[u8]) -> bool {
    let mut c = cart();
    let mut p = Ppu::new();
    p.oam.fill(0xff);
    p.oam[..layout.len()].copy_from_slice(layout);
    p.mask = 0x18;
    to(&mut p, &mut c, 0, 0);
    p.oam_addr = oam_addr;
    to(&mut p, &mut c, 1, 0);
    p.status & 0x20 != 0
}

#[test]
fn misaligned_oam_realigns_on_out_of_range_y_and_x() {
    let sprite = [0x00, 0xe3, 0x00, 0x00];
    // +1: Y in range keeps the offset; +4 & $FC: an out-of-range Y realigns.
    let mut y_realign = vec![0xff];
    for _ in 0..6 {
        y_realign.extend(sprite);
    }
    y_realign.extend([0x80, 0xff, 0xff]);
    for _ in 0..3 {
        y_realign.extend(sprite);
    }
    assert!(overflow_after_misaligned_start(1, &y_realign));
    // Secondary OAM full: an out-of-range Y adds 5 instead.
    let mut plus_five = vec![0xff];
    for _ in 0..8 {
        plus_five.extend(sprite);
    }
    plus_five.extend([0x80, 0xff, 0xff, 0xff, 0xff, 0x00]);
    assert!(overflow_after_misaligned_start(1, &plus_five));
    // An out-of-range X position adds 1 & $FC, realigning early.
    let mut x_realign = vec![0xff];
    for _ in 0..6 {
        x_realign.extend(sprite);
    }
    x_realign.extend([0x00, 0xe3, 0x00, 0x80, 0xe3, 0xff, 0xff]);
    for _ in 0..2 {
        x_realign.extend(sprite);
    }
    assert!(overflow_after_misaligned_start(1, &x_realign));
    // Offset by two: the X check realigns onto the byte after the X position.
    let mut off2 = vec![0xff, 0xff];
    for i in 0..7u8 {
        off2.extend([0x00, 0xe3, 0x10 * (i + 1), 0x00]);
    }
    off2.extend([0x00, 0xe3, 0x00, 0x80, 0x00]);
    assert!(overflow_after_misaligned_start(2, &off2));
    // Offset by three with a full secondary OAM: +1 & $FC, then +5.
    let mut off3 = vec![0xff, 0xff, 0xff];
    for i in 0..7u8 {
        off3.extend([0x00, 0x10 * (i + 1), 0x00, 0x00]);
    }
    off3.extend([0x00, 0x80, 0x00, 0x80, 0xff, 0xff, 0x00]);
    assert!(overflow_after_misaligned_start(3, &off3));
}

#[test]
fn first_sprite_evaluated_becomes_sprite_zero() {
    let mut c = cart();
    let mut p = Ppu::new();
    // Tile 1 is solid for both background and sprite.
    for row in 0..8 {
        chr(&mut c, 16 + row, 0xff);
    }
    p.vram[0] = 1;
    p.oam.fill(0xff);
    p.oam[0x80..0x84].copy_from_slice(&[0, 1, 0, 0]);
    p.mask = 0x1e;
    to(&mut p, &mut c, 0, 0);
    p.oam_addr = 0x80;
    to(&mut p, &mut c, 1, 20);
    assert_ne!(
        p.status & 0x40,
        0,
        "OAM index 32 processed first must act as sprite zero"
    );
}

#[test]
fn oam2_address_freezes_after_overflow_until_an_enabled_reset_point() {
    let mut c = cart();
    let mut p = Ppu::new();
    // Eight in-range sprites with distinct Y bytes fill secondary OAM on line 9.
    p.oam.fill(0xff);
    for i in 0..8 {
        p.oam[i * 4..i * 4 + 4].copy_from_slice(&[9 - i as u8, 0x10 + i as u8, 0, 0x20 * i as u8]);
    }
    p.mask = 0x18;
    // Disable during evaluation after the fill, re-enable for the fetch: the
    // overflow latch never cleared at dot 256, so every fetch reads slot 0.
    to(&mut p, &mut c, 9, 240);
    p.mask = 0;
    to(&mut p, &mut c, 9, 258);
    p.mask = 0x18;
    to(&mut p, &mut c, 9, 330);
    assert_eq!(
        p.read_register(0x2004, c.mapper.as_mut()),
        9,
        "frozen address keeps reading OAM2[0]"
    );
    // Normal operation: the address wraps to 0 at dot 321 and reads slot 0
    // during 321-340 for the following line too.
    let mut c = cart();
    let mut p = Ppu::new();
    p.oam.fill(0xff);
    p.oam[..4].copy_from_slice(&[0x5a, 0, 0, 0]);
    p.mask = 0x18;
    to(&mut p, &mut c, 91, 330);
    assert_eq!(p.read_register(0x2004, c.mapper.as_mut()), 0x5a);
    // Pausing rendering inside the fetch leaves the address behind.
    let mut c = cart();
    let mut p = Ppu::new();
    p.oam.fill(0xff);
    for i in 0..8u8 {
        p.oam[i as usize * 4..i as usize * 4 + 4].copy_from_slice(&[
            i,
            0x01 + 0x20 * i,
            0x02 + 0x20 * i,
            0x03 + 0x20 * i,
        ]);
    }
    p.mask = 0x18;
    to(&mut p, &mut c, 7, 268);
    p.mask = 0;
    to(&mut p, &mut c, 7, 286);
    p.mask = 0x18;
    to(&mut p, &mut c, 7, 324);
    assert_eq!(
        p.read_register(0x2004, c.mapper.as_mut()),
        6,
        "18 missing dots leave OAM2 address at $18"
    );
}

#[test]
fn pre_render_fetch_treats_stale_secondary_oam_as_line_five() {
    let mut c = cart();
    let mut p = Ppu::new();
    // Sprite tile 6 has one pixel on row 5; background tile 0 has one at x=128.
    chr(&mut c, 6 * 16 + 5, 0x80);
    chr(&mut c, 16, 0x80);
    p.vram[16] = 1;
    p.oam.fill(0);
    p.oam[..4].copy_from_slice(&[0, 6, 0, 128]);
    p.mask = 0x1e;
    // Ordinary rendering: Y=0 appears on line 1, never on line 0.
    to(&mut p, &mut c, 240, 0);
    assert_eq!(p.status & 0x40, 0);
    // Blank the frame after line 0's evaluation filled secondary OAM, then
    // enable only after the pre-render clear window: the stale slot 0 is
    // fetched as if the line were 5 and drawn on scanline 0.
    let mut c = cart();
    let mut p = Ppu::new();
    chr(&mut c, 6 * 16 + 5, 0x80);
    chr(&mut c, 16, 0x80);
    p.vram[16] = 1;
    p.oam.fill(0);
    p.oam[..4].copy_from_slice(&[0, 6, 0, 128]);
    p.mask = 0x1e;
    to(&mut p, &mut c, 0, 250);
    p.mask = 0;
    to(&mut p, &mut c, 261, 100);
    p.mask = 0x1e;
    to(&mut p, &mut c, 1, 0);
    assert_ne!(
        p.status & 0x40,
        0,
        "stale secondary OAM draws on scanline 0"
    );
}

#[test]
fn ppudata_read_fills_the_buffer_from_the_fetch_five_dots_later() {
    let mut c = cart();
    let mut p = Ppu::new();
    for i in 0..1024 {
        p.vram[i] = i as u8;
    }
    for i in 0..64 {
        p.vram[960 + i] = 0xc0 + i as u8;
    }
    p.mask = 0x08;
    // A read whose state machine lands on the nametable fetch of dot 2.
    to(&mut p, &mut c, 4, 338);
    p.read_register(0x2007, c.mapper.as_mut());
    ticks(&mut p, &mut c, 5);
    assert_eq!((p.scanline, p.dot), (5, 2));
    p.mask = 0;
    let buffered = p.read_register(0x2007, c.mapper.as_mut());
    assert_eq!(
        buffered, 2,
        "buffer holds the tile fetched at dot 2 of the next line"
    );
    // Landing on the attribute read instead.
    let mut c = cart();
    let mut p = Ppu::new();
    for i in 0..1024 {
        p.vram[i] = i as u8;
    }
    for i in 0..64 {
        p.vram[960 + i] = 0xc0 + i as u8;
    }
    p.mask = 0x08;
    to(&mut p, &mut c, 5, 7);
    p.read_register(0x2007, c.mapper.as_mut());
    ticks(&mut p, &mut c, 5);
    assert_eq!(p.dot, 12);
    p.mask = 0;
    assert_eq!(p.read_register(0x2007, c.mapper.as_mut()), 0xc0);
    // Outside rendering the buffer comes from v, five dots after the access,
    // and v increments afterwards.
    let mut c = cart();
    let mut p = Ppu::new();
    p.vram[0x123] = 0x87;
    set_v(&mut p, &mut c, 0x2123);
    p.read_register(0x2007, c.mapper.as_mut());
    ticks(&mut p, &mut c, 4);
    assert_eq!(p.v, 0x2123);
    ticks(&mut p, &mut c, 1);
    assert_eq!(p.v, 0x2124);
    assert_eq!(p.read_register(0x2007, c.mapper.as_mut()), 0x87);
}

#[test]
fn ppudata_read_during_a_pattern_latch_corrupts_the_next_fetch() {
    // ALE + Read: the octal latch keeps the data byte, so the low plane comes
    // from $0FFF instead of the tile's row.
    let mut c = cart();
    let mut p = Ppu::new();
    // Tile $F0 is blank, but its pattern rows live at $0F00-$0F0F, so the
    // hybrid address becomes $0FFF.
    p.vram.fill(0xf0);
    for i in 0..64 {
        p.vram[960 + i] = 0xff;
    }
    chr(&mut c, 0x0fff, 0xff);
    p.palette[0x0d] = 0x21;
    p.mask = 0x08;
    // The read at dot 224 latches at 227 and reads at 229, the low-plane latch.
    to(&mut p, &mut c, 3, 224);
    p.read_register(0x2007, c.mapper.as_mut());
    to(&mut p, &mut c, 3, 340);
    let line = &p.framebuffer[3 * 256..4 * 256];
    assert_eq!(
        &line[240..248],
        &[0x21; 8],
        "eight pixels of palette 3 color 1"
    );
    assert_eq!(line[239], 0);
    assert_eq!(line[248], 0);
}

#[test]
fn late_ppuaddr_copy_makes_a_hybrid_nametable_fetch() {
    let mut c = cart();
    let mut p = Ppu::new();
    // Tile 2 has a single pixel on row 2 (the fine Y the copied $2F00 selects);
    // the tile sits at $2F19 only.
    chr(&mut c, 2 * 16 + 2, 0x80);
    p.vram[0x719] = 2; // $2F19 with horizontal mirroring lives in bank 1
    p.palette[1] = 0x21;
    p.mask = 0x08;
    // Fetch of coarse X 25 on line 4: latch at 185, read at 186. The copy
    // written at dot 183 lands on 186 and only replaces the high byte.
    to(&mut p, &mut c, 4, 183);
    p.w = true;
    p.t = 0x2f00;
    p.write_register(0x2006, 0x00, c.mapper.as_mut());
    to(&mut p, &mut c, 4, 340);
    let line = &p.framebuffer[4 * 256..5 * 256];
    assert_eq!(line[200], 0x21, "tile from $2F19 drawn at x=200");
    assert_eq!(line[199], 0);
}

#[test]
fn disabling_rendering_mid_line_copies_oam_row_zero_when_it_resumes() {
    let mut c = cart();
    let mut p = Ppu::new();
    p.oam.fill(0xff);
    p.oam[..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    p.mask = 0x18;
    // Write lands three dots later: rendering stops on dot 9 of the pre-render
    // line, where the secondary OAM clear is at index 4.
    to(&mut p, &mut c, 261, 6);
    p.write_register(0x2001, 0, c.mapper.as_mut());
    to(&mut p, &mut c, 261, 200);
    assert_eq!(
        &p.oam[32..40],
        &[0xff; 8],
        "no corruption until rendering resumes"
    );
    p.write_register(0x2001, 0x18, c.mapper.as_mut());
    to(&mut p, &mut c, 261, 210);
    assert_eq!(&p.oam[32..40], &[1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(&p.oam[40..48], &[0xff; 8]);
    // Re-enabling only during vblank does not copy.
    let mut c = cart();
    let mut p = Ppu::new();
    p.oam.fill(0xff);
    p.oam[..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    p.mask = 0x18;
    to(&mut p, &mut c, 10, 6);
    p.write_register(0x2001, 0, c.mapper.as_mut());
    to(&mut p, &mut c, 245, 0);
    p.write_register(0x2001, 0x18, c.mapper.as_mut());
    ticks(&mut p, &mut c, 6);
    p.write_register(0x2001, 0, c.mapper.as_mut());
    to(&mut p, &mut c, 260, 0);
    assert!(p.oam[8..].iter().all(|&b| b == 0xff));
}

#[test]
fn status_read_clears_sprite_flags_on_the_dot_after_the_vblank_sample() {
    let mut c = cart();
    let mut p = Ppu::new();
    p.status = 0xe0;
    to(&mut p, &mut c, 261, 0);
    let early = p.read_register(0x2002, c.mapper.as_mut());
    assert_eq!(
        early & 0xe0,
        0xe0,
        "all flags still set when M2 rises on dot 0"
    );
    p.tick(c.mapper.as_mut());
    assert_eq!(
        p.status & 0xe0,
        0,
        "dot 1 clears the flags before the read ends"
    );
}

#[test]
fn status_read_finishes_with_sprite_flags_one_dot_later() {
    // Bus wiring resamples the sprite flags after the read cycle's last dot:
    // the access lands after pre-render dot 0, the last dot is dot 1's clear.
    let mut bus = bus();
    bus.ppu.status = 0xe0;
    bus.ppu.scanline = 260;
    bus.ppu.dot = 339;
    assert_eq!(
        bus.read(0x2002) & 0xe0,
        0x80,
        "vblank at M2 rise, sprite flags after the clear"
    );
    assert_eq!(
        bus.read(0x2000) & 0xe0,
        0x80,
        "PPU open bus keeps the finished value"
    );
}

#[test]
fn odd_frame_skip_emits_the_first_sprite_pixel_at_x_zero() {
    let mut c = cart();
    let mut p = Ppu::new();
    chr(&mut c, 6 * 16 + 5, 0x80);
    p.palette[0x11] = 0x27;
    p.oam.fill(0);
    p.oam[..4].copy_from_slice(&[0, 6, 0, 128]);
    // Line 0's evaluation leaves sprite 0 in secondary OAM; rendering then
    // stays off except for the end of each pre-render line and line 0.
    p.mask = 0x1e;
    to(&mut p, &mut c, 0, 250);
    p.mask = 0;
    let mut seen = Vec::new();
    for _ in 0..4 {
        to(&mut p, &mut c, 261, 100);
        p.mask = 0x1e;
        to(&mut p, &mut c, 1, 0);
        seen.push((p.framebuffer[0] == 0x27, p.framebuffer[128] == 0x27));
        p.mask = 0;
    }
    assert!(
        seen.contains(&(true, false)) && seen.contains(&(false, true)),
        "{seen:?}"
    );
}

#[test]
fn sprite_overflow_scan_wraps_after_eight_visible_sprites() {
    let mut c = cart();
    let mut p = Ppu::new();
    p.scanline = 21;
    p.mask = 0x18;
    p.oam.fill(0xff);
    for i in 0..8 {
        p.oam[i * 4..i * 4 + 4].copy_from_slice(&[20, 0, 0, 0]);
    }
    ticks(&mut p, &mut c, 256);
    assert_eq!(p.oam_addr, 32);
    assert_eq!(p.status & 0x20, 0);
}

#[test]
fn partially_cleared_oam_finishes_the_sprite_when_it_fills() {
    let mut c = cart();
    let mut p = Ppu::new();
    p.scanline = 21;
    p.mask = 0x18;
    p.oam.fill(20);
    ticks(&mut p, &mut c, 59);
    p.write_register(0x2001, 0, c.mapper.as_mut());
    ticks(&mut p, &mut c, 4);
    p.write_register(0x2001, 0x18, c.mapper.as_mut());
    ticks(&mut p, &mut c, 30);
    assert_eq!(p.dot, 93);
    assert_ne!(p.status & 0x20, 0);
}
