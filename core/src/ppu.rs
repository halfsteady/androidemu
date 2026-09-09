//! Picture Processing Unit (2C02).
//!
//! Dot-driven background fetches and scrolling, with scanline sprite selection.
//! Sprite evaluation is functional, but does not yet reproduce the overflow bug
//! or cycle-exact secondary OAM accesses.

use crate::cart::{mapper::Mapper, Mirroring, Region};

pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 240;

/// Dots per scanline. Every region shares this; they differ in scanline count,
/// in the scanline vblank starts on, and in the PPU-to-CPU clock ratio, all of
/// which come from [`Region`].
const DOTS_PER_SCANLINE: u16 = 341;

#[derive(Clone)]
pub struct Ppu {
    pub(crate) region: Region,
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

    pub vram: [u8; 4096],
    pub palette: [u8; 32],

    pub dot: u16,
    pub scanline: u16,
    pub frame: u64,
    total_dots: u64,
    suppress_vblank: bool,
    odd_skip: bool,
    /// Set while the PPU is asserting /NMI.
    pub nmi_line: bool,

    /// Six-bit NES palette indices. Color emphasis is not encoded yet.
    pub framebuffer: Vec<u8>,
    bg_low: u16,
    bg_high: u16,
    attr_low: u16,
    attr_high: u16,
    tile: u8,
    attribute: u8,
    pattern_low: u8,
    pattern_high: u8,
    sprites: [Sprite; 8],
    sprite_count: usize,
}

#[derive(Clone, Copy, Default)]
struct Sprite {
    x: u8,
    attributes: u8,
    low: u8,
    high: u8,
    zero: bool,
    address: u16,
}

impl Default for Ppu {
    fn default() -> Self {
        Ppu::new()
    }
}

impl Ppu {
    pub fn new() -> Ppu {
        Ppu {
            region: Region::Ntsc,
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
            vram: [0; 4096],
            palette: [0; 32],
            dot: 0,
            scanline: 0,
            frame: 0,
            total_dots: 0,
            suppress_vblank: false,
            odd_skip: false,
            nmi_line: false,
            framebuffer: vec![0; WIDTH * HEIGHT],
            bg_low: 0,
            bg_high: 0,
            attr_low: 0,
            attr_high: 0,
            tile: 0,
            attribute: 0,
            pattern_low: 0,
            pattern_high: 0,
            sprites: [Sprite::default(); 8],
            sprite_count: 0,
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
    pub fn tick(&mut self, mapper: &mut dyn Mapper) {
        self.total_dots = self.total_dots.wrapping_add(1);
        // The final pre-render dot is omitted on odd frames while rendering.
        if self.region == Region::Ntsc && self.scanline == (self.region.scanlines() - 1)
            && self.dot == 339
            && self.frame & 1 != 0
            && self.odd_skip
        {
            self.dot = 0;
            self.scanline = 0;
            self.frame += 1;
            return;
        }
        self.dot += 1;
        if self.dot >= DOTS_PER_SCANLINE {
            self.dot = 0;
            self.scanline += 1;
            if self.scanline >= self.region.scanlines() {
                self.scanline = 0;
                self.frame += 1;
            }
        }

        if (self.scanline < 240 || self.scanline == (self.region.scanlines() - 1)) && self.rendering() {
            if (2..=257).contains(&self.dot) || (322..=337).contains(&self.dot) {
                self.bg_low = (self.bg_low << 1) | 1;
                self.bg_high = (self.bg_high << 1) | 1;
                self.attr_low <<= 1;
                self.attr_high <<= 1;
            }
            if (1..=256).contains(&self.dot) || (321..=336).contains(&self.dot) {
                match self.dot % 8 {
                    1 => {
                        self.load_background();
                        self.tile = self.memory_read(0x2000 | (self.v & 0x0fff), mapper);
                    }
                    3 => {
                        let addr = 0x23c0
                            | (self.v & 0x0c00)
                            | ((self.v >> 4) & 0x38)
                            | ((self.v >> 2) & 7);
                        let shift = ((self.v >> 4) & 4) | (self.v & 2);
                        self.attribute = (self.memory_read(addr, mapper) >> shift) & 3;
                    }
                    5 => self.pattern_low = self.memory_read(self.pattern_address(), mapper),
                    7 => self.pattern_high = self.memory_read(self.pattern_address() + 8, mapper),
                    0 => self.increment_x(),
                    _ => {}
                }
            }
            if self.dot == 256 {
                self.increment_y();
            }
            if self.dot == 257 {
                self.load_background();
                self.v = (self.v & !0x041f) | (self.t & 0x041f);
                self.select_sprites();
            }
            if self.scanline == (self.region.scanlines() - 1) && (280..=304).contains(&self.dot) {
                self.v = (self.v & !0x7be0) | (self.t & 0x7be0);
            }
            if (257..=320).contains(&self.dot) {
                // Sprite fetch forces the primary OAM address to zero. Without
                // this, a CPU write to OAMADDR misaligns every subsequent DMA.
                self.oam_addr = 0;
                let slot = ((self.dot - 257) / 8) as usize;
                match self.dot % 8 {
                    1 | 3 => {
                        self.memory_read(0x2000 | (self.v & 0xfff), mapper);
                    }
                    5 | 7 => {
                        let address =
                            self.sprites[slot].address + if self.dot % 8 == 7 { 8 } else { 0 };
                        let value = self.memory_read(address, mapper);
                        if self.dot % 8 == 5 {
                            self.sprites[slot].low = value;
                        } else {
                            self.sprites[slot].high = value;
                        }
                    }
                    _ => {}
                }
            }
            if self.dot == 337 || self.dot == 339 {
                if self.dot == 337 {
                    self.load_background();
                }
                self.memory_read(0x2000 | (self.v & 0xfff), mapper);
            }
        }

        if self.scanline < 240 && (1..=256).contains(&self.dot) {
            self.render_pixel();
        }

        if self.scanline == (self.region.scanlines() - 1) && self.dot == 338 {
            self.odd_skip = self.rendering();
        }
        if self.scanline == self.region.vblank_scanline() && self.dot == 1 {
            if !self.suppress_vblank {
                self.status |= 0x80;
            }
            self.suppress_vblank = false;
            self.update_nmi();
        }
        if self.scanline == (self.region.scanlines() - 1) && self.dot == 1 {
            // Vblank, sprite 0 hit and sprite overflow all clear together.
            self.status &= !0xE0;
            self.update_nmi();
        }
    }

    // ---- CPU-facing registers, $2000-$2007 mirrored to $3FFF ----

    pub fn read_register(&mut self, addr: u16, mapper: &mut dyn Mapper) -> u8 {
        match addr & 7 {
            2 => {
                if self.scanline == self.region.vblank_scanline() && self.dot == 0 {
                    self.suppress_vblank = true;
                }
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
                let v = if self.rendering() && self.scanline < 240 && (1..=64).contains(&self.dot) {
                    0xff // Secondary OAM is being cleared.
                } else {
                    self.oam[self.oam_addr as usize]
                        & if self.oam_addr & 3 == 2 { 0xe3 } else { 0xff }
                };
                self.open_bus = v;
                v
            }
            7 => {
                let addr = self.v & 0x3FFF;
                let v = if addr >= 0x3F00 {
                    // Palette reads are immediate, but the buffer is still loaded
                    // with the nametable byte underneath.
                    self.read_buffer = self.memory_read(addr - 0x1000, mapper);
                    (self.palette_read(addr) & if self.mask & 1 != 0 { 0x30 } else { 0x3f })
                        | (self.open_bus & 0xc0)
                } else {
                    let buffered = self.read_buffer;
                    self.read_buffer = self.memory_read(addr, mapper);
                    buffered
                };
                self.increment_v();
                mapper.ppu_bus(self.v & 0x3fff, self.total_dots);
                self.open_bus = v;
                v
            }
            // Write-only registers return the open bus.
            _ => self.open_bus,
        }
    }

    pub fn write_register(&mut self, addr: u16, val: u8, mapper: &mut dyn Mapper) {
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
                if self.rendering() && (self.scanline < 240 || self.scanline == self.region.scanlines() - 1) {
                    // Only the sprite index (high six bits) increments; the
                    // byte offset within the sprite is retained.
                    self.oam_addr = self.oam_addr.wrapping_add(4);
                } else {
                    self.oam[self.oam_addr as usize] = val;
                    self.oam_addr = self.oam_addr.wrapping_add(1);
                }
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
                    mapper.ppu_bus(self.v & 0x3fff, self.total_dots);
                }
                self.w = !self.w;
            }
            7 => {
                let addr = self.v & 0x3FFF;
                if addr >= 0x3F00 {
                    self.palette_write(addr, val);
                } else {
                    self.memory_write(addr, val, mapper);
                }
                self.increment_v();
                mapper.ppu_bus(self.v & 0x3fff, self.total_dots);
            }
            _ => {}
        }
    }

    fn increment_v(&mut self) {
        if self.rendering() && (self.scanline < 240 || self.scanline == (self.region.scanlines() - 1)) {
            self.increment_x();
            self.increment_y();
            return;
        }
        let step = if self.ctrl & 0x04 != 0 { 32 } else { 1 };
        self.v = self.v.wrapping_add(step) & 0x7FFF;
    }

    fn rendering(&self) -> bool {
        self.mask & 0x18 != 0
    }

    fn pattern_address(&self) -> u16 {
        ((self.ctrl as u16 & 0x10) << 8) | (self.tile as u16 * 16) | ((self.v >> 12) & 7)
    }

    fn load_background(&mut self) {
        self.bg_low = (self.bg_low & 0xff00) | self.pattern_low as u16;
        self.bg_high = (self.bg_high & 0xff00) | self.pattern_high as u16;
        self.attr_low = (self.attr_low & 0xff00) | if self.attribute & 1 != 0 { 0xff } else { 0 };
        self.attr_high = (self.attr_high & 0xff00) | if self.attribute & 2 != 0 { 0xff } else { 0 };
    }

    fn increment_x(&mut self) {
        if self.v & 0x1f == 31 {
            self.v = (self.v & !0x1f) ^ 0x400;
        } else {
            self.v += 1;
        }
    }

    fn increment_y(&mut self) {
        if self.v & 0x7000 != 0x7000 {
            self.v += 0x1000;
            return;
        }
        self.v &= !0x7000;
        let mut y = (self.v >> 5) & 31;
        if y == 29 {
            y = 0;
            self.v ^= 0x800;
        } else if y == 31 {
            y = 0;
        } else {
            y += 1;
        }
        self.v = (self.v & !0x3e0) | (y << 5);
    }

    fn select_sprites(&mut self) {
        self.sprite_count = 0;
        self.sprites.fill(Sprite {
            address: if self.ctrl & 0x20 != 0 {
                0x1ff0
            } else {
                ((self.ctrl as u16 & 8) << 9) | 0xff0
            },
            ..Sprite::default()
        });
        let next = if self.scanline == (self.region.scanlines() - 1) {
            0
        } else {
            self.scanline + 1
        };
        let height = if self.ctrl & 0x20 != 0 { 16 } else { 8 };
        for i in 0..64 {
            // OAM Y is the scanline above the sprite; $EF-$FF are offscreen.
            let row = next as i16 - self.oam[i * 4] as i16 - 1;
            if row < 0 || row >= height {
                continue;
            }
            if self.sprite_count == 8 {
                self.status |= 0x20;
                break;
            }
            let tile = self.oam[i * 4 + 1];
            let attributes = self.oam[i * 4 + 2];
            let row = if attributes & 0x80 != 0 {
                height - 1 - row
            } else {
                row
            } as u16;
            let addr = if height == 16 {
                ((tile as u16 & 1) << 12) | (((tile as u16 & 0xfe) + row / 8) << 4) | (row & 7)
            } else {
                ((self.ctrl as u16 & 8) << 9) | (tile as u16 * 16) | row
            };
            self.sprites[self.sprite_count] = Sprite {
                x: self.oam[i * 4 + 3],
                attributes,
                low: 0,
                high: 0,
                zero: i == 0,
                address: addr,
            };
            self.sprite_count += 1;
        }
    }

    fn render_pixel(&mut self) {
        let x = self.dot as usize - 1;
        let bit = 0x8000 >> self.x;
        let background = if self.mask & 8 != 0 && (x >= 8 || self.mask & 2 != 0) {
            u8::from(self.bg_low & bit != 0) | (u8::from(self.bg_high & bit != 0) << 1)
        } else {
            0
        };
        let attribute =
            u8::from(self.attr_low & bit != 0) | (u8::from(self.attr_high & bit != 0) << 1);
        let mut index = if background != 0 {
            attribute * 4 + background
        } else {
            0
        };
        if self.mask & 0x10 != 0 && (x >= 8 || self.mask & 4 != 0) {
            for sprite in &self.sprites[..self.sprite_count] {
                let offset = x as i16 - sprite.x as i16;
                if !(0..8).contains(&offset) {
                    continue;
                }
                let shift = if sprite.attributes & 0x40 != 0 {
                    offset
                } else {
                    7 - offset
                };
                let pixel = ((sprite.low >> shift) & 1) | (((sprite.high >> shift) & 1) << 1);
                if pixel == 0 {
                    continue;
                }
                if sprite.zero && background != 0 && x != 255 {
                    self.status |= 0x40;
                }
                if background == 0 || sprite.attributes & 0x20 == 0 {
                    index = 0x10 | ((sprite.attributes & 3) * 4) | pixel;
                }
                break;
            }
        }
        let addr = if !self.rendering() && self.v & 0x3f00 == 0x3f00 {
            self.v
        } else {
            0x3f00 + index as u16
        };
        let color = self.palette_read(addr) & if self.mask & 1 != 0 { 0x30 } else { 0x3f };
        self.framebuffer[self.scanline as usize * WIDTH + x] = color;
    }

    fn nametable_index(&self, addr: u16, mapper: &dyn Mapper) -> usize {
        let offset = (addr as usize - 0x2000) & 0xfff;
        let table = offset / 0x400;
        let bank = match mapper.mirroring() {
            Mirroring::Horizontal => table / 2,
            Mirroring::Vertical => table % 2,
            Mirroring::SingleScreenLo => 0,
            Mirroring::SingleScreenHi => 1,
            Mirroring::FourScreen => table,
        };
        bank * 0x400 + offset % 0x400
    }

    fn memory_read(&mut self, addr: u16, mapper: &mut dyn Mapper) -> u8 {
        mapper.ppu_bus(addr & 0x3fff, self.total_dots);
        match addr & 0x3fff {
            a @ 0..=0x1fff => mapper.ppu_read(a),
            a @ 0x2000..=0x3eff => self.vram[self.nametable_index(a, mapper)],
            a => self.palette_read(a),
        }
    }

    fn memory_write(&mut self, addr: u16, val: u8, mapper: &mut dyn Mapper) {
        mapper.ppu_bus(addr & 0x3fff, self.total_dots);
        match addr & 0x3fff {
            a @ 0..=0x1fff => mapper.ppu_write(a, val),
            a @ 0x2000..=0x3eff => {
                let i = self.nametable_index(a, mapper);
                self.vram[i] = val;
            }
            a => self.palette_write(a, val),
        }
    }

    fn palette_read(&self, addr: u16) -> u8 {
        self.palette[palette_index(addr)]
    }
    fn palette_write(&mut self, addr: u16, val: u8) {
        self.palette[palette_index(addr)] = val & 0x3f;
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

crate::state::state_fields!(
    Ppu,
    ctrl,
    mask,
    status,
    oam_addr,
    oam,
    v,
    t,
    x,
    w,
    read_buffer,
    open_bus,
    vram,
    palette,
    dot,
    scanline,
    frame,
    total_dots,
    suppress_vblank,
    odd_skip,
    nmi_line,
    framebuffer,
    bg_low,
    bg_high,
    attr_low,
    attr_high,
    tile,
    attribute,
    pattern_low,
    pattern_high,
    sprites,
    sprite_count;
    region: Region::Ntsc
);
crate::state::state_fields!(Sprite, x, attributes, low, high, zero, address);
impl Ppu {
    pub(crate) fn valid_state(&self) -> bool {
        self.x < 8
            && self.dot < 341
            && self.scanline < self.region.scanlines()
            && self.sprite_count <= 8
            && self.framebuffer.len() == WIDTH * HEIGHT
            && self.sprites.iter().all(|s| s.address <= 0x1ff7)
    }
}
