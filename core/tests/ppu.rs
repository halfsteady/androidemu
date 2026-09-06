use nes_core::{cart::Cartridge, ppu::Ppu};

fn cart(flags: u8) -> Cartridge {
    let mut rom = vec![0; 16 + 16384];
    rom[..4].copy_from_slice(b"NES\x1a");
    rom[4] = 1;
    rom[6] = flags;
    Cartridge::load(&rom).unwrap()
}
fn write(p: &mut Ppu, c: &mut Cartridge, addr: u16, value: u8) {
    p.write_register(6, (addr >> 8) as u8, c.mapper.as_mut());
    p.write_register(6, addr as u8, c.mapper.as_mut());
    p.write_register(7, value, c.mapper.as_mut());
}
fn read(p: &mut Ppu, c: &mut Cartridge, addr: u16) -> u8 {
    p.write_register(6, (addr >> 8) as u8, c.mapper.as_mut());
    p.write_register(6, addr as u8, c.mapper.as_mut());
    let v = p.read_register(7, c.mapper.as_mut());
    if addr >= 0x3f00 {
        v
    } else {
        p.read_register(7, c.mapper.as_mut())
    }
}
fn frame(p: &mut Ppu, c: &mut Cartridge) -> usize {
    let target = p.frame + 1;
    let mut dots = 0;
    while p.frame != target {
        p.tick(c.mapper.as_mut());
        dots += 1;
    }
    dots
}
fn scene() -> (Ppu, Cartridge) {
    let mut p = Ppu::new();
    let mut c = cart(0);
    p.oam.fill(0xff);
    // Tile 0: alternating opaque/transparent pixels; tile 1: solid color 2.
    for row in 0..8 {
        write(&mut p, &mut c, row, 0xaa);
        write(&mut p, &mut c, 24 + row, 0xff);
    }
    write(&mut p, &mut c, 0x3f00, 0x0f);
    write(&mut p, &mut c, 0x3f01, 0x21);
    write(&mut p, &mut c, 0x3f12, 0x32);
    p.v = 0;
    p.t = 0;
    p.x = 0;
    p.mask = 0x1e;
    (p, c)
}
#[test]
fn ppudata_chr_buffering_and_increment() {
    let mut p = Ppu::new();
    let mut c = cart(0);
    write(&mut p, &mut c, 0x123, 0x87);
    assert_eq!(read(&mut p, &mut c, 0x123), 0x87);
    p.write_register(0, 4, c.mapper.as_mut());
    write(&mut p, &mut c, 0x2000, 42);
    assert_eq!(p.v, 0x2020);
    p.write_register(7, 43, c.mapper.as_mut());
    assert_eq!(read(&mut p, &mut c, 0x2020), 43);
}
#[test]
fn nametable_mirroring_and_3000_alias() {
    for (flags, banks) in [(0, [0, 0, 1, 1]), (1, [0, 1, 0, 1]), (8, [0, 1, 2, 3])] {
        let mut p = Ppu::new();
        let mut c = cart(flags);
        for table in 0..4 {
            write(&mut p, &mut c, 0x2000 + table * 0x400, table as u8 + 1);
        }
        for table in 0..4 {
            let last = (0..4).rev().find(|&i| banks[i] == banks[table]).unwrap();
            assert_eq!(
                read(&mut p, &mut c, 0x3000 + table as u16 * 0x400),
                last as u8 + 1
            );
        }
    }
}
#[test]
fn palette_is_immediate_mirrored_and_refills_buffer() {
    let mut p = Ppu::new();
    let mut c = cart(0);
    write(&mut p, &mut c, 0x2f10, 0x55);
    write(&mut p, &mut c, 0x3f10, 0xff);
    assert_eq!(read(&mut p, &mut c, 0x3f10) & 0x3f, 0x3f);
    assert_eq!(p.palette[0], 0x3f);
    p.write_register(6, 0x20, c.mapper.as_mut());
    p.write_register(6, 0, c.mapper.as_mut());
    assert_eq!(p.read_register(7, c.mapper.as_mut()), 0x55);
}
#[test]
fn background_pixels_fine_scroll_and_left_clipping() {
    for fine in 0..8 {
        let (mut p, mut c) = scene();
        p.x = fine;
        frame(&mut p, &mut c);
        frame(&mut p, &mut c);
        for x in 0..256 {
            assert_eq!(
                p.framebuffer[20 * 256 + x],
                if (x + fine as usize).is_multiple_of(2) {
                    0x21
                } else {
                    0x0f
                },
                "x={x}, fine={fine}"
            );
        }
    }
    let (mut p, mut c) = scene();
    p.mask &= !2;
    frame(&mut p, &mut c);
    frame(&mut p, &mut c);
    assert_eq!(&p.framebuffer[256..264], &[0x0f; 8]);
    assert_eq!(p.framebuffer[264], 0x21);
}
#[test]
fn sprites_priority_zero_hit_and_eight_sprite_limit() {
    let (mut p, mut c) = scene();
    p.oam[..4].copy_from_slice(&[9, 1, 0x20, 16]);
    frame(&mut p, &mut c);
    // Hit clears at pre-render, so inspect during the visible region.
    while p.scanline != 11 {
        p.tick(c.mapper.as_mut());
    }
    assert_ne!(p.status & 0x40, 0);
    assert_eq!(p.framebuffer[10 * 256 + 16], 0x21);
    assert_eq!(p.framebuffer[10 * 256 + 17], 0x32);
    p.mask = 0;
    p.oam.fill(0xff);
    for i in 0..9 {
        p.oam[i * 4..i * 4 + 4].copy_from_slice(&[19, 1, 0, i as u8 * 8]);
    }
    p.mask = 0x14;
    while p.scanline != 21 {
        p.tick(c.mapper.as_mut());
    }
    assert_ne!(p.status & 0x20, 0);
    assert_eq!(p.framebuffer[20 * 256 + 63], 0x32);
    assert_eq!(p.framebuffer[20 * 256 + 64], 0x0f);
}
#[test]
fn ntsc_frame_lengths_and_vblank_nmi() {
    let mut p = Ppu::new();
    let mut c = cart(0);
    assert_eq!(frame(&mut p, &mut c), 89342);
    assert_eq!(frame(&mut p, &mut c), 89342);
    p.mask = 8;
    assert_eq!(frame(&mut p, &mut c), 89342);
    assert_eq!(frame(&mut p, &mut c), 89341);
    p.ctrl = 0x80;
    while !(p.scanline == 241 && p.dot == 1) {
        p.tick(c.mapper.as_mut());
    }
    assert!(p.nmi_line);
    assert_ne!(p.read_register(2, c.mapper.as_mut()) & 0x80, 0);
    assert!(!p.nmi_line);
}

#[test]
fn scroll_crosses_nametables_and_attributes_choose_palette() {
    let mut p = Ppu::new();
    // Four-screen storage makes each table independently observable.
    let mut c = cart(8);
    for row in 0..8 {
        write(&mut p, &mut c, row, 0xff);
        write(&mut p, &mut c, 24 + row, 0xff);
    }
    for i in 0..960 {
        write(&mut p, &mut c, 0x2400 + i, 1);
    }
    write(&mut p, &mut c, 0x23c7, 0x04); // top-right quadrant of final attribute block
    write(&mut p, &mut c, 0x3f05, 0x15);
    write(&mut p, &mut c, 0x3f02, 0x22);
    p.t = 31;
    p.v = 31;
    p.x = 3;
    p.mask = 0x0a;
    frame(&mut p, &mut c);
    frame(&mut p, &mut c);
    assert_eq!(&p.framebuffer[..5], &[0x15; 5]);
    assert_eq!(&p.framebuffer[5..16], &[0x22; 11]);
}

#[test]
fn tall_sprites_use_tile_bank_and_both_flip_bits() {
    let mut p = Ppu::new();
    let mut c = cart(0);
    p.oam.fill(0xff);
    // 8x16 tile 1 selects bank $1000, tiles 0/1. Last row's left
    // pixel becomes the top row's right pixel when flipped both ways.
    write(&mut p, &mut c, 0x1017, 0x80);
    write(&mut p, &mut c, 0x3f11, 0x27);
    write(&mut p, &mut c, 0x3f00, 0x0f);
    p.oam[..4].copy_from_slice(&[9, 1, 0xc0, 20]);
    p.ctrl = 0x20;
    p.mask = 0x14;
    p.v = 0;
    p.t = 0;
    frame(&mut p, &mut c);
    assert_eq!(p.framebuffer[10 * 256 + 20], 0x0f);
    assert_eq!(p.framebuffer[10 * 256 + 27], 0x27);
    assert_eq!(p.framebuffer[11 * 256 + 27], 0x0f);
}

#[test]
fn cpu_bus_routes_ppudata_to_cartridge() {
    use nes_core::{cpu::Bus, NesBus};
    let mut bus = NesBus::new(cart(0));
    bus.write(0x2006, 0x01);
    bus.write(0x2006, 0x23);
    bus.write(0x2007, 0x5a);
    bus.write(0x2006, 0x01);
    bus.write(0x2006, 0x23);
    bus.read(0x2007);
    assert_eq!(bus.read(0x2007), 0x5a);
}
