//! Region timing. NTSC, PAL and Dendy differ in CPU clock, PPU divider, frame
//! height and the scanline vblank begins on; each of those is observable from
//! the assembled machine.
mod common;
use nes_core::{Nes, Region};

/// The shared test ROM, re-headered for a region. iNES 1.0 signals PAL in byte 9;
/// Dendy only exists in NES 2.0, in the low bits of byte 12.
fn rom(region: Region) -> Vec<u8> {
    let mut rom = common::rom(0, 1);
    match region {
        Region::Ntsc => {}
        Region::Pal => rom[9] = 1,
        Region::Dendy => {
            rom[7] |= 0x08;
            rom[12] = 3;
        }
    }
    rom
}

/// Cycles the CPU spends in one frame, averaged over enough frames that the
/// instruction granularity at each frame boundary washes out.
fn cycles_per_frame(nes: &mut Nes) -> f64 {
    for _ in 0..2 {
        nes.step_frame();
    }
    let start = nes.cpu.cycles;
    for _ in 0..20 {
        nes.step_frame();
    }
    (nes.cpu.cycles - start) as f64 / 20.0
}

#[test]
fn headers_select_the_region() {
    for region in [Region::Ntsc, Region::Pal, Region::Dendy] {
        assert_eq!(Nes::new(&rom(region)).unwrap().bus.cart.header.region, region);
    }
}

#[test]
fn each_region_runs_its_own_frame_length() {
    // 341 dots x scanlines, divided by the PPU-to-CPU ratio: 3 on NTSC and Dendy,
    // 3.2 on PAL. Rendering is disabled here, so NTSC never skips its odd dot.
    for (region, expected) in [
        (Region::Ntsc, 341.0 * 262.0 / 3.0),
        (Region::Pal, 341.0 * 312.0 / 3.2),
        (Region::Dendy, 341.0 * 312.0 / 3.0),
    ] {
        let mut nes = Nes::new(&rom(region)).unwrap();
        let measured = cycles_per_frame(&mut nes);
        assert!(
            (measured - expected).abs() < 2.0,
            "{region:?}: {measured} cycles per frame, expected about {expected}"
        );
    }
}

#[test]
fn frame_height_and_vblank_scanline_follow_the_region() {
    for region in [Region::Ntsc, Region::Pal, Region::Dendy] {
        let mut nes = Nes::new(&rom(region)).unwrap();
        nes.bus.ppu.ctrl = 0x80;
        nes.step_frame();

        let mut highest = 0;
        let mut vblank_at = None;
        let target = nes.bus.ppu.frame + 1;
        while nes.bus.ppu.frame < target {
            let had_vblank = nes.bus.ppu.status & 0x80 != 0;
            nes.step();
            highest = highest.max(nes.bus.ppu.scanline);
            if !had_vblank && nes.bus.ppu.status & 0x80 != 0 {
                vblank_at = Some(nes.bus.ppu.scanline);
                assert!(nes.bus.ppu.nmi_line, "{region:?}: vblank did not raise NMI");
            }
        }
        assert_eq!(highest, region.scanlines() - 1, "{region:?}: frame height");
        assert_eq!(vblank_at, Some(region.vblank_scanline()), "{region:?}: vblank");
    }
}

#[test]
fn savestates_restore_region_timing() {
    // Region is cartridge metadata rather than serialized device state, so a
    // reloaded machine has to pick it back up from the header.
    for region in [Region::Pal, Region::Dendy] {
        let mut nes = Nes::new(&rom(region)).unwrap();
        for _ in 0..3000 {
            nes.step();
        }
        let saved = nes.save_state();
        let mut restored = Nes::new(&rom(region)).unwrap();
        restored.load_state(&saved).unwrap();
        assert_eq!(restored.save_state(), saved);
        assert!((cycles_per_frame(&mut restored) - 341.0 * 312.0 / if region == Region::Pal { 3.2 } else { 3.0 }).abs() < 2.0);
    }
}
