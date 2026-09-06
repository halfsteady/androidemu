//! Picture Processing Unit (2C02).
//!
//! Phase 0 scope: the register interface, the frame/scanline timing that drives
//! vblank and NMI, and open-bus decay behaviour. Background and sprite rendering
//! land in Phase 1 (see PLAN.md §12); the framebuffer exists and is cleared so the
//! rest of the pipeline can be built against a real surface.

pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 240;

/// Dots per scanline, and scanlines per frame, for NTSC.
const DOTS_PER_SCANLINE: u16 = 341;
const SCANLINES_PER_FRAME: u16 = 262;
/// The scanline on which vblank is entered and the NMI may fire.
const VBLANK_SCANLINE: u16 = 241;
/// The pre-render scanline, where flags are cleared.
const PRERENDER_SCANLINE: u16 = 261;

pub struct Ppu {
    /// $2000 PPUCTRL
    pub ctrl: u8,
    /// $2001 PPUMASK
    pub mask: u8,
    /// $2002 PPUSTATUS
    pub status: u8,
    /// $2003 OAMADDR
    pub oam_addr: u8,
    pub oam: [u8; 256],

    /// Loopy's internal registers: v (current VRAM address), t (temporary),
    /// x (fine X scroll), w (write toggle).
    pub v: u16,
    pub t: u16,
    pub x: u8,
    pub w: bool,

    /// The delayed read buffer behind $2007.
    read_buffer: u8,
    /// Value on the PPU's data bus, which decays but is not modelled as decaying yet.
    open_bus: u8,

    pub vram: [u8; 2048],
    pub palette: [u8; 32],

    pub dot: u16,
    pub scanline: u16,
    pub frame: u64,
    /// Set while the PPU is asserting /NMI.
    pub nmi_line: bool,

    pub framebuffer: Vec<u8>,
}

impl Default for Ppu {
    fn default() -> Self {
        Ppu::new()
    }
}

impl Ppu {
    pub fn new() -> Ppu {
        Ppu {
            ctrl: 0,
            mask: 0,
            status: 0,
            oam_addr: 0,
            oam: [0; 256],
            v: 0,
            t: 0,
            x: 0,
            w: false,
            read_buffer: 0,
            open_bus: 0,
            vram: [0; 2048],
            palette: [0; 32],
            dot: 0,
            scanline: 0,
            frame: 0,
            nmi_line: false,
            framebuffer: vec![0; WIDTH * HEIGHT],
        }
    }

    #[inline]
    fn nmi_enabled(&self) -> bool {
        self.ctrl & 0x80 != 0
    }

    #[inline]
    fn in_vblank(&self) -> bool {
        self.status & 0x80 != 0
    }

    fn update_nmi(&mut self) {
        self.nmi_line = self.nmi_enabled() && self.in_vblank();
    }

    /// Advance one PPU dot. Called three times per CPU cycle on NTSC.
    pub fn tick(&mut self) {
        self.dot += 1;
        if self.dot >= DOTS_PER_SCANLINE {
            self.dot = 0;
            self.scanline += 1;
            if self.scanline >= SCANLINES_PER_FRAME {
                self.scanline = 0;
                self.frame += 1;
            }
        }

        if self.scanline == VBLANK_SCANLINE && self.dot == 1 {
            self.status |= 0x80;
            self.update_nmi();
        }
        if self.scanline == PRERENDER_SCANLINE && self.dot == 1 {
            // Vblank, sprite 0 hit and sprite overflow all clear together.
            self.status &= !0xE0;
            self.update_nmi();
        }
    }

    // ---- CPU-facing registers, $2000-$2007 mirrored to $3FFF ----

    pub fn read_register(&mut self, addr: u16) -> u8 {
        match addr & 7 {
            2 => {
                // Reading PPUSTATUS clears vblank and resets the write toggle. The
                // low five bits come from whatever was last on the data bus.
                let v = (self.status & 0xE0) | (self.open_bus & 0x1F);
                self.status &= !0x80;
                self.w = false;
                self.update_nmi();
                self.open_bus = v;
                v
            }
            4 => {
                let v = self.oam[self.oam_addr as usize];
                self.open_bus = v;
                v
            }
            7 => {
                let addr = self.v & 0x3FFF;
                let v = if addr >= 0x3F00 {
                    // Palette reads are immediate, but the buffer is still loaded
                    // with the nametable byte underneath.
                    self.read_buffer = self.vram_read(addr - 0x1000);
                    self.palette_read(addr)
                } else {
                    let buffered = self.read_buffer;
                    self.read_buffer = self.vram_read(addr);
                    buffered
                };
                self.increment_v();
                self.open_bus = v;
                v
            }
            // Write-only registers return the open bus.
            _ => self.open_bus,
        }
    }

    pub fn write_register(&mut self, addr: u16, val: u8) {
        self.open_bus = val;
        match addr & 7 {
            0 => {
                self.ctrl = val;
                self.t = (self.t & 0xF3FF) | (((val as u16) & 0x03) << 10);
                self.update_nmi();
            }
            1 => self.mask = val,
            3 => self.oam_addr = val,
            4 => {
                self.oam[self.oam_addr as usize] = val;
                self.oam_addr = self.oam_addr.wrapping_add(1);
            }
            5 => {
                if !self.w {
                    self.t = (self.t & 0xFFE0) | ((val as u16) >> 3);
                    self.x = val & 0x07;
                } else {
                    self.t = (self.t & 0x8FFF) | (((val as u16) & 0x07) << 12);
                    self.t = (self.t & 0xFC1F) | (((val as u16) & 0xF8) << 2);
                }
                self.w = !self.w;
            }
            6 => {
                if !self.w {
                    self.t = (self.t & 0x00FF) | (((val as u16) & 0x3F) << 8);
                } else {
                    self.t = (self.t & 0xFF00) | val as u16;
                    self.v = self.t;
                }
                self.w = !self.w;
            }
            7 => {
                let addr = self.v & 0x3FFF;
                if addr >= 0x3F00 {
                    self.palette_write(addr, val);
                } else {
                    self.vram_write(addr, val);
                }
                self.increment_v();
            }
            _ => {}
        }
    }

    fn increment_v(&mut self) {
        let step = if self.ctrl & 0x04 != 0 { 32 } else { 1 };
        self.v = self.v.wrapping_add(step) & 0x7FFF;
    }

    // ---- PPU address space. Pattern tables live on the cartridge and are wired
    // ---- up by the bus; these two cover VRAM and palette only.

    fn vram_read(&self, _addr: u16) -> u8 {
        0
    }
    fn vram_write(&mut self, _addr: u16, _val: u8) {}

    fn palette_read(&self, addr: u16) -> u8 {
        self.palette[palette_index(addr)]
    }
    fn palette_write(&mut self, addr: u16, val: u8) {
        self.palette[palette_index(addr)] = val;
    }
}

/// $3F10/$14/$18/$1C mirror the corresponding background entries.
fn palette_index(addr: u16) -> usize {
    let mut i = (addr as usize) & 0x1F;
    if i & 0x13 == 0x10 {
        i &= !0x10;
    }
    i
}
