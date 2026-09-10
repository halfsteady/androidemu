//! Picture Processing Unit (2C02).
//!
//! Dot-driven background and sprite pipelines. Sprite evaluation is cycle
//! exact: secondary OAM is cleared, filled through the OAM data buffer and
//! read back by the sprite fetch with the same address counters the hardware
//! uses, including misaligned OAM, the overflow bug and the frozen secondary
//! address. Memory accesses take two dots (address latch, then read) so CPU
//! accesses to $2006/$2007 can corrupt fetches the way the hardware does.
//!
//! Timing conventions (see docs/ACCURACY.md): a CPU access sees the PPU state
//! after the dot on which M2 rises. Values the hardware latches when M2 falls
//! (sprite flags of $2002) are finished one dot later by [`Ppu::finish_status_read`].
//! Register writes to PPUMASK and the second PPUADDR write take effect two dots
//! after M2 falls, three dots after the access.

use crate::cart::{mapper::Mapper, Mirroring, Region};

pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 240;

/// Dots per scanline. Every region shares this; they differ in scanline count,
/// in the scanline vblank starts on, and in the PPU-to-CPU clock ratio, all of
/// which come from [`Region`].
const DOTS_PER_SCANLINE: u16 = 341;
// Charge retention is analogue and varies between PPUs. Use a deterministic
// roughly 1/3-second interval, within the measured range, independently per bit.
const OPEN_BUS_DECAY_DOTS: u64 = 341 * 262 * 20;
/// PPUMASK and the PPUADDR copy into v settle two dots after the write cycle
/// ends, which is three dots after the emulated access point.
const WRITE_SETTLE_DOTS: u8 = 3;
/// The PPUDATA state machine after the CPU access: address latch enable,
/// then the read (or the write, one dot sooner).
const DATA_ALE_DOT: u8 = 3;
const DATA_WRITE_DOT: u8 = 4;
const DATA_READ_DOT: u8 = 5;

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
    /// CPU-facing PPU data latch and the last driven time of each bit.
    open_bus: u8,
    open_bus_refreshed: [u64; 8],

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
    /// The eight sprite output units. `x` counts down to the sprite's first
    /// pixel; `low`/`high` are the live shifters.
    sprites: [Sprite; 8],
    sprite_count: usize,

    // ---- v3 pipeline state ----
    pending_mask: u8,
    mask_delay: u8,
    pending_v: u16,
    v_delay: u8,
    /// Units whose X counter is still counting down (not yet outputting).
    counting: [bool; 8],
    /// Per-unit fetch scratch: the Y and tile bytes read from secondary OAM,
    /// and whether the range check let the pattern planes load.
    sprite_y: [u8; 8],
    sprite_tile: [u8; 8],
    sprite_loaded: [bool; 8],
    /// Secondary OAM and its address counter. `oam2_full` is the overflow
    /// latch that freezes the counter until the next enabled reset point.
    oam2: [u8; 32],
    oam2_addr: u8,
    oam2_full: bool,
    /// Whatever the sprite hardware last put on the OAM data bus; $2004 reads
    /// return it while rendering.
    oam_buffer: u8,
    eval_in_range: bool,
    eval_done: bool,
    eval_bytes: u8,
    overflow_bytes: u8,
    /// Slot 0 of secondary OAM holds the sprite that gets sprite-zero hits.
    oam2_sprite0: bool,
    /// Rows of the address bus: the external octal latch and the last data
    /// byte seen on the shared low pins.
    bus_latch: u8,
    bus_high: u8,
    bus_data: u8,
    data_ops: [DataOp; 2],
    corrupt_arm: bool,
    corrupt_pending: bool,
    corrupt_row: u8,
    skip_glitch: bool,
    /// Sprite 0 hit and overflow detected on the previous dot; they become
    /// visible in PPUSTATUS one dot after the dot that detects them.
    pending_status: u8,
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

/// One pending PPUDATA access working its way through the state machine.
#[derive(Clone, Copy, Default)]
struct DataOp {
    /// 0: idle, 1: read, 2: write, 3: address only (palette write).
    kind: u8,
    addr: u16,
    value: u8,
    age: u8,
}

crate::state::state_fields!(DataOp, kind, addr, value, age);

/// A background or sprite fetch, resolved to an address when it is latched or read.
#[derive(Clone, Copy, PartialEq)]
enum Fetch {
    Nametable,
    Attribute,
    PatternLow,
    PatternHigh,
    SpriteLow(usize),
    SpriteHigh(usize),
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
            open_bus_refreshed: [0; 8],
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
            pending_mask: 0,
            mask_delay: 0,
            pending_v: 0,
            v_delay: 0,
            counting: [false; 8],
            sprite_y: [0; 8],
            sprite_tile: [0; 8],
            sprite_loaded: [false; 8],
            oam2: [0xff; 32],
            oam2_addr: 0,
            oam2_full: false,
            oam_buffer: 0xff,
            eval_in_range: false,
            eval_done: false,
            eval_bytes: 0,
            overflow_bytes: 0,
            oam2_sprite0: false,
            bus_latch: 0,
            bus_high: 0,
            bus_data: 0,
            data_ops: [DataOp::default(); 2],
            corrupt_arm: false,
            corrupt_pending: false,
            corrupt_row: 0,
            skip_glitch: false,
            pending_status: 0,
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

    #[inline]
    fn pre_render_line(&self) -> u16 {
        self.region.scanlines() - 1
    }

    /// Visible lines and the pre-render line run the fetch pipelines.
    #[inline]
    fn render_line(&self) -> bool {
        self.scanline < 240 || self.scanline == self.pre_render_line()
    }

    fn rendering(&self) -> bool {
        self.mask & 0x18 != 0
    }

    fn sprite_height(&self) -> i16 {
        if self.ctrl & 0x20 != 0 {
            16
        } else {
            8
        }
    }

    /// Advance one PPU dot. Called three times per CPU cycle on NTSC.
    pub fn tick(&mut self, mapper: &mut dyn Mapper) {
        self.total_dots = self.total_dots.wrapping_add(1);
        self.status |= self.pending_status;
        self.pending_status = 0;
        self.settle_writes(mapper);
        // The final pre-render dot is omitted on odd frames while rendering.
        if self.region == Region::Ntsc
            && self.scanline == self.pre_render_line()
            && self.dot == 339
            && self.frame & 1 != 0
            && self.odd_skip
        {
            self.dot = 0;
            self.scanline = 0;
            self.frame += 1;
            self.skip_glitch = true;
            self.bus_step(None, mapper);
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

        let render_line = self.render_line();
        let rendering = self.rendering();
        if render_line && rendering && self.corrupt_pending {
            self.corrupt_oam();
        }
        if render_line && !rendering && self.corrupt_arm {
            // Rendering just stopped in the middle of sprite processing: the
            // row copied when it resumes depends on where the hardware was.
            self.corrupt_arm = false;
            self.corrupt_pending = true;
            self.corrupt_row = self.corruption_row();
        } else if !render_line {
            self.corrupt_arm = false;
        }

        if self.scanline < 240 && (1..=256).contains(&self.dot) {
            self.render_pixel();
        }

        if render_line && rendering {
            self.run_pipelines(mapper);
        } else {
            self.bus_step(None, mapper);
        }

        if self.scanline == self.pre_render_line() && self.dot == 338 {
            // The skip is decided from the register the CPU has already
            // written, even while the pipeline still runs on the old value.
            let mask = if self.mask_delay != 0 {
                self.pending_mask
            } else {
                self.mask
            };
            self.odd_skip = mask & 0x18 != 0;
        }
        if self.scanline == self.region.vblank_scanline() && self.dot == 1 {
            if !self.suppress_vblank {
                self.status |= 0x80;
            }
            self.suppress_vblank = false;
            self.update_nmi();
        }
        if self.scanline == self.pre_render_line() && self.dot == 1 {
            // Vblank, sprite 0 hit and sprite overflow all clear together.
            self.status &= !0xE0;
            self.update_nmi();
        }
    }

    /// Delayed register effects land at the start of a dot.
    fn settle_writes(&mut self, mapper: &mut dyn Mapper) {
        if self.mask_delay > 0 {
            self.mask_delay -= 1;
            if self.mask_delay == 0 {
                let was = self.rendering();
                self.mask = self.pending_mask;
                if was && !self.rendering() {
                    self.corrupt_arm = true;
                }
            }
        }
        if self.v_delay > 0 {
            self.v_delay -= 1;
            if self.v_delay == 0 {
                self.v = self.pending_v;
                if !(self.rendering() && self.render_line()) {
                    mapper.ppu_bus(self.v & 0x3fff, self.total_dots);
                }
            }
        }
    }

    /// Background fetches, sprite evaluation and sprite fetches for one dot of
    /// a rendering line.
    fn run_pipelines(&mut self, mapper: &mut dyn Mapper) {
        let dot = self.dot;
        let pre_render = self.scanline == self.pre_render_line();

        // Background: shift after the pixel was output, load a new tile once
        // its high plane arrives, and fetch the next one over eight dots.
        if (1..=256).contains(&dot) || (321..=336).contains(&dot) {
            self.bg_low <<= 1;
            self.bg_high = (self.bg_high << 1) | 1;
            self.attr_low <<= 1;
            self.attr_high <<= 1;
        }
        let bg_fetch = (1..=256).contains(&dot) || (321..=336).contains(&dot);
        let sprite_fetch = (257..=320).contains(&dot);
        let action = if bg_fetch {
            match dot % 8 {
                1 => Some((false, Fetch::Nametable)),
                2 => Some((true, Fetch::Nametable)),
                3 => Some((false, Fetch::Attribute)),
                4 => Some((true, Fetch::Attribute)),
                5 => Some((false, Fetch::PatternLow)),
                6 => Some((true, Fetch::PatternLow)),
                7 => Some((false, Fetch::PatternHigh)),
                _ => Some((true, Fetch::PatternHigh)),
            }
        } else if sprite_fetch {
            let slot = ((dot - 257) / 8) as usize;
            match (dot - 257) % 8 {
                0 | 2 => Some((false, Fetch::Nametable)),
                1 | 3 => Some((true, Fetch::Nametable)),
                4 => Some((false, Fetch::SpriteLow(slot))),
                5 => Some((true, Fetch::SpriteLow(slot))),
                6 => Some((false, Fetch::SpriteHigh(slot))),
                _ => Some((true, Fetch::SpriteHigh(slot))),
            }
        } else if dot == 337 || dot == 339 {
            Some((false, Fetch::Nametable))
        } else if dot == 338 || dot == 340 {
            Some((true, Fetch::Nametable))
        } else {
            None
        };

        if sprite_fetch {
            // Sprite fetch forces the primary OAM address to zero. Without
            // this, a CPU write to OAMADDR misaligns every subsequent DMA.
            self.oam_addr = 0;
            if dot == 257 {
                self.oam2_addr = 0;
            }
            self.sprite_fetch_dot(pre_render);
        }

        let data = self.bus_step(action, mapper);
        if let Some((true, fetch)) = action {
            match fetch {
                Fetch::Nametable if bg_fetch => self.tile = data,
                Fetch::Attribute => {
                    let shift = ((self.v >> 4) & 4) | (self.v & 2);
                    self.attribute = (data >> shift) & 3;
                }
                Fetch::PatternLow => self.pattern_low = data,
                Fetch::PatternHigh => {
                    self.pattern_high = data;
                    self.increment_x();
                    self.load_background();
                }
                Fetch::SpriteLow(slot) => {
                    self.sprites[slot].low = self.sprite_plane(slot, data);
                }
                Fetch::SpriteHigh(slot) => {
                    self.sprites[slot].high = self.sprite_plane(slot, data);
                    self.oam2_read(true);
                }
                _ => {}
            }
        }
        if dot == 256 {
            self.increment_y();
        }
        if dot == 257 {
            self.v = (self.v & !0x041f) | (self.t & 0x041f);
        }
        if pre_render && (280..=304).contains(&dot) {
            self.v = (self.v & !0x7be0) | (self.t & 0x7be0);
        }

        // Secondary OAM: clear, evaluate, then hand over to the fetch.
        if (1..=64).contains(&dot) {
            self.oam2_addr = ((dot - 1) / 2) as u8;
            self.oam_buffer = 0xff;
            self.oam2[self.oam2_addr as usize] = 0xff;
        } else if (65..=256).contains(&dot) && !pre_render {
            if dot == 65 {
                self.oam2_addr = 0;
                self.eval_in_range = false;
                self.eval_done = false;
                self.eval_bytes = 0;
                self.overflow_bytes = 0;
                self.oam2_sprite0 = false;
            }
            if dot & 1 != 0 {
                self.oam_buffer = self.oam_read(self.oam_addr);
            } else {
                self.evaluate_sprite(dot);
            }
        } else if !sprite_fetch {
            self.oam_buffer = self.oam2[(self.oam2_addr & 31) as usize];
        }
        if dot == 64 || dot == 256 || dot == 339 {
            self.oam2_full = false;
        }
        if dot == 339 {
            // Units switch to counting; a missed pulse leaves them outputting.
            self.counting = std::array::from_fn(|i| self.sprites[i].x != 0);
        }
    }

    /// One even evaluation dot: act on the byte read on the previous dot.
    fn evaluate_sprite(&mut self, dot: u16) {
        let value = self.oam_buffer;
        let height = self.sprite_height();
        let line = self.scanline as i16;
        let in_range = |y: u8| {
            let row = line - y as i16;
            row >= 0 && row < height
        };
        if self.eval_done {
            // All 64 sprites seen: keep reading Y positions, but the write to
            // secondary OAM fails and shows its current byte instead.
            self.oam_buffer = self.oam2[(self.oam2_addr & 31) as usize];
            self.oam_addr = self.oam_addr.wrapping_add(4);
            return;
        }
        if !self.oam2_full {
            self.oam2[(self.oam2_addr & 31) as usize] = value;
            if self.eval_in_range {
                self.eval_bytes += 1;
                self.oam_addr = self.oam_addr.wrapping_add(1);
                self.oam2_increment();
                if self.eval_bytes == 4 {
                    self.eval_in_range = false;
                    self.eval_bytes = 0;
                    // The X position goes through the same range check; a
                    // miss realigns a misaligned OAM address to the next sprite.
                    if !in_range(value) {
                        self.oam_addr &= 0xfc;
                    }
                    if self.oam_addr >> 2 == 0 {
                        self.eval_done = true;
                    }
                }
            } else if in_range(value) {
                self.eval_in_range = true;
                self.eval_bytes = 1;
                if dot == 66 {
                    self.oam2_sprite0 = true;
                }
                self.oam_addr = self.oam_addr.wrapping_add(1);
                self.oam2_increment();
            } else {
                self.oam_addr = self.oam_addr.wrapping_add(4) & 0xfc;
                if self.oam_addr == 0 {
                    self.eval_done = true;
                }
            }
        } else {
            // Secondary OAM is full: the write turns into a read, and the
            // hardware's diagonal scan produces the sprite overflow bug.
            self.oam_buffer = self.oam2[(self.oam2_addr & 31) as usize];
            if self.eval_in_range && self.eval_bytes != 0 {
                // If a partially cleared secondary OAM filled mid-sprite,
                // finish that copy's remaining bytes before starting overflow
                // evaluation. Full OAM blocks writes, not the primary counter.
                self.eval_bytes += 1;
                self.oam_addr = self.oam_addr.wrapping_add(1);
                if self.eval_bytes == 4 {
                    self.eval_bytes = 0;
                    self.eval_in_range = false;
                    if !in_range(value) {
                        self.oam_addr &= 0xfc;
                    }
                    if self.oam_addr >> 2 == 0 {
                        self.eval_done = true;
                    }
                }
            } else if self.eval_in_range {
                self.overflow_bytes -= 1;
                if self.overflow_bytes == 0 {
                    self.oam_addr &= 0xfc;
                    self.eval_in_range = false;
                    self.eval_done = true;
                } else {
                    self.oam_addr = self.oam_addr.wrapping_add(1);
                }
            } else if in_range(value) {
                // Three more bytes are read (m carrying into n), then the
                // address realigns to the start of the sprite it landed in.
                self.pending_status |= 0x20;
                self.eval_in_range = true;
                self.overflow_bytes = 3;
                self.oam_addr = self.oam_addr.wrapping_add(1);
            } else {
                let n = ((self.oam_addr >> 2) + 1) & 0x3f;
                let m = self.oam_addr.wrapping_add(1) & 3;
                self.oam_addr = (n << 2) | m;
                if n == 0 {
                    self.eval_done = true;
                }
            }
        }
    }

    fn oam2_increment(&mut self) {
        if self.oam2_full {
            return;
        }
        self.oam2_addr += 1;
        if self.oam2_addr >= 32 {
            self.oam2_addr = 0;
            self.oam2_full = true;
        }
    }

    /// Read the byte the fetch is looking at in secondary OAM, optionally
    /// stepping the address counter afterwards.
    fn oam2_read(&mut self, increment: bool) -> u8 {
        let value = self.oam2[(self.oam2_addr & 31) as usize];
        self.oam_buffer = value;
        if increment {
            self.oam2_increment();
        }
        value
    }

    /// Dots 257-320: load the output units from secondary OAM, eight dots per
    /// unit. The pre-render line compares against line 5 (261 & 255).
    fn sprite_fetch_dot(&mut self, pre_render: bool) {
        let slot = ((self.dot - 257) / 8) as usize;
        match (self.dot - 257) % 8 {
            0 => self.sprite_y[slot] = self.oam2_read(true),
            1 => self.sprite_tile[slot] = self.oam2_read(true),
            2 => {
                self.sprites[slot].attributes = self.oam2_read(true);
                self.sprites[slot].zero = slot == 0 && self.oam2_sprite0;
            }
            3 => self.sprites[slot].x = self.oam2_read(false),
            4 => {
                self.oam2_read(false);
                let y = self.sprite_y[slot];
                let tile = self.sprite_tile[slot];
                let attributes = self.sprites[slot].attributes;
                let height = self.sprite_height();
                let line = if pre_render { 5 } else { self.scanline as i16 };
                let row = line - y as i16;
                // Out-of-range units still fetch, but load transparent planes.
                self.sprite_loaded[slot] = row >= 0 && row < height;
                let mut row = (row as u16) & (height as u16 - 1);
                if attributes & 0x80 != 0 {
                    row = height as u16 - 1 - row;
                }
                self.sprites[slot].address = if height == 16 {
                    ((tile as u16 & 1) << 12) | (((tile as u16 & 0xfe) + row / 8) << 4) | (row & 7)
                } else {
                    ((self.ctrl as u16 & 8) << 9) | ((tile as u16) << 4) | row
                };
            }
            5..=7 => {
                self.oam2_read(false);
            }
            _ => unreachable!(),
        }
    }

    /// A fetched pattern plane as it enters the unit's shifter: transparent
    /// when the range check failed, bit-reversed for horizontal flips so the
    /// shifter always emits from bit 7.
    fn sprite_plane(&self, slot: usize, data: u8) -> u8 {
        if !self.sprite_loaded[slot] {
            0
        } else if self.sprites[slot].attributes & 0x40 != 0 {
            data.reverse_bits()
        } else {
            data
        }
    }

    /// The seed of an OAM row copy: where the secondary OAM machinery was when
    /// rendering was switched off.
    fn corruption_row(&self) -> u8 {
        let dot = self.dot;
        if (1..=64).contains(&dot) {
            ((dot - 1) / 2) as u8
        } else if (65..=256).contains(&dot) {
            if self.oam2_full {
                0
            } else {
                ((self.oam2_addr + 3) & 0x1c) & 0x1f
            }
        } else if (257..=320).contains(&dot) {
            let step = ((dot - 257) % 8).min(3) as u8;
            ((dot - 257) / 8) as u8 * 4 + step
        } else {
            0
        }
    }

    fn corrupt_oam(&mut self) {
        self.corrupt_pending = false;
        let row = (self.corrupt_row & 31) as usize;
        if row != 0 {
            let first: [u8; 8] = self.oam[..8].try_into().unwrap();
            self.oam[row * 8..row * 8 + 8].copy_from_slice(&first);
        }
        let target = (self.oam2_addr & 31) as usize;
        self.oam2[target] = self.oam2[0];
    }

    /// One dot of the shared address/data bus. `action` is the fetch pipeline's
    /// activity this dot (latch an address, or read the latched one); the
    /// PPUDATA state machine rides along and conflicts with it as on hardware.
    fn bus_step(&mut self, action: Option<(bool, Fetch)>, mapper: &mut dyn Mapper) -> u8 {
        let mut sm_ale = None;
        let mut sm_read = false;
        let mut sm_write = None;
        let mut completed = 0;
        for i in 0..self.data_ops.len() {
            let op = &mut self.data_ops[i];
            if op.kind == 0 {
                continue;
            }
            op.age += 1;
            match (op.kind, op.age) {
                (_, DATA_ALE_DOT) => {
                    // The address is taken from v when the latch enable fires,
                    // so a read that follows a still-pending one (a DMA
                    // repeating the CPU's read) already sees the increment.
                    op.addr = self.v & 0x3fff;
                    sm_ale = Some(op.addr);
                }
                (1, DATA_READ_DOT) => {
                    sm_read = true;
                    op.kind = 0;
                    completed += 1;
                }
                (2, DATA_WRITE_DOT) => {
                    sm_write = Some(op.value);
                    op.kind = 0;
                    completed += 1;
                }
                (3, DATA_WRITE_DOT) => {
                    op.kind = 0;
                    completed += 1;
                }
                _ => {}
            }
        }
        let mut result = 0;
        match action {
            Some((false, fetch)) => {
                let addr = self.fetch_address(fetch);
                self.bus_latch = addr as u8;
                self.bus_high = (addr >> 8) as u8;
                mapper.ppu_bus(addr & 0x3fff, self.total_dots);
                if sm_read {
                    // Read and address-latch in the same dot: the latch takes
                    // the data still on the shared pins, and the fetch that
                    // follows uses that low byte too.
                    let latch = self.bus_data;
                    let hybrid = ((addr & 0xff00) | latch as u16) & 0x3fff;
                    let value = self.bus_read(hybrid, mapper);
                    self.read_buffer = value;
                    self.bus_latch = value;
                    self.bus_data = value;
                }
                if let Some(value) = sm_write {
                    let hybrid = ((addr & 0xff00) | self.bus_latch as u16) & 0x3fff;
                    self.bus_write(hybrid, value, mapper);
                }
            }
            Some((true, fetch)) => {
                let now = self.fetch_address(fetch);
                if let Some(sm) = sm_ale {
                    // The PPU drives its own low byte while the fetch reads.
                    self.bus_latch = sm as u8;
                }
                let addr = ((now & 0xff00) | self.bus_latch as u16) & 0x3fff;
                let value = self.bus_read(addr, mapper);
                self.bus_data = value;
                result = value;
                if sm_read {
                    self.read_buffer = value;
                }
                if let Some(v) = sm_write {
                    self.bus_write(addr, v, mapper);
                }
            }
            None => {
                if let Some(sm) = sm_ale {
                    self.bus_latch = sm as u8;
                    self.bus_high = (sm >> 8) as u8;
                    mapper.ppu_bus(sm & 0x3fff, self.total_dots);
                }
                let addr = (((self.bus_high as u16) << 8) | self.bus_latch as u16) & 0x3fff;
                if sm_read {
                    let value = self.bus_read(addr, mapper);
                    self.read_buffer = value;
                    self.bus_data = value;
                }
                if let Some(value) = sm_write {
                    self.bus_write(addr, value, mapper);
                }
            }
        }
        // The address increments once the access has finished, so a fetch on
        // the same dot still sees the old address. In blanking the bus then
        // shows the new one.
        for _ in 0..completed {
            self.increment_v();
        }
        if completed > 0 && action.is_none() {
            mapper.ppu_bus(self.v & 0x3fff, self.total_dots);
        }
        result
    }

    fn fetch_address(&self, fetch: Fetch) -> u16 {
        match fetch {
            Fetch::Nametable => 0x2000 | (self.v & 0x0fff),
            Fetch::Attribute => {
                0x23c0 | (self.v & 0x0c00) | ((self.v >> 4) & 0x38) | ((self.v >> 2) & 7)
            }
            Fetch::PatternLow => self.pattern_address(),
            Fetch::PatternHigh => self.pattern_address() + 8,
            Fetch::SpriteLow(slot) => self.sprites[slot].address,
            Fetch::SpriteHigh(slot) => self.sprites[slot].address + 8,
        }
    }

    // ---- CPU-facing registers, $2000-$2007 mirrored to $3FFF ----

    pub fn read_register(&mut self, addr: u16, mapper: &mut dyn Mapper) -> u8 {
        // Memory is touched by the PPUDATA state machine in later ticks; the
        // mapper stays in the signature for callers and future bus quirks.
        let _ = mapper;
        self.decay_open_bus();
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
                self.drive_open_bus(v, 0xe0);
                v
            }
            4 => {
                let v = if self.rendering() && self.render_line() {
                    self.oam_buffer
                } else {
                    self.oam_read(self.oam_addr)
                };
                self.drive_open_bus(v, 0xff);
                v
            }
            7 => {
                let addr = self.v & 0x3FFF;
                let v = if addr >= 0x3F00 {
                    // Palette reads are immediate; the buffer is still loaded
                    // from the nametable underneath by the bus read.
                    (self.palette_read(addr) & if self.mask & 1 != 0 { 0x30 } else { 0x3f })
                        | (self.open_bus & 0xc0)
                } else {
                    self.read_buffer
                };
                self.queue_data_op(1, 0);
                self.drive_open_bus(v, if addr >= 0x3f00 { 0x3f } else { 0xff });
                v
            }
            // Write-only registers return the open bus.
            _ => self.open_bus,
        }
    }

    /// The sprite flags of a PPUSTATUS read are sampled when the CPU read cycle
    /// ends, one dot after the vblank flag. The bus calls this after that dot.
    pub(crate) fn finish_status_read(&mut self, early: u8) -> u8 {
        let value = (early & !0x60) | (self.status & 0x60);
        self.drive_open_bus(value, 0x60);
        value
    }

    pub fn write_register(&mut self, addr: u16, val: u8, mapper: &mut dyn Mapper) {
        self.drive_open_bus(val, 0xff);
        match addr & 7 {
            0 => {
                self.ctrl = val;
                self.t = (self.t & 0xF3FF) | (((val as u16) & 0x03) << 10);
                self.update_nmi();
            }
            1 => {
                self.pending_mask = val;
                self.mask_delay = WRITE_SETTLE_DOTS;
            }
            3 => self.oam_addr = val,
            4 => {
                if self.rendering() && self.render_line() {
                    // Only the sprite index (high six bits) increments, and the
                    // byte offset is cleared.
                    self.oam_addr = self.oam_addr.wrapping_add(4) & 0xfc;
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
                    self.pending_v = self.t;
                    self.v_delay = WRITE_SETTLE_DOTS;
                }
                self.w = !self.w;
            }
            7 => {
                let addr = self.v & 0x3FFF;
                if addr >= 0x3F00 {
                    self.palette_write(addr, val);
                    self.queue_data_op(3, val);
                } else {
                    self.queue_data_op(2, val);
                }
            }
            _ => {}
        }
        let _ = mapper;
    }

    fn queue_data_op(&mut self, kind: u8, value: u8) {
        let slot = self
            .data_ops
            .iter()
            .position(|op| op.kind == 0)
            .unwrap_or(0);
        self.data_ops[slot] = DataOp {
            kind,
            addr: 0,
            value,
            age: 0,
        };
    }

    fn increment_v(&mut self) {
        if self.rendering() && self.render_line() {
            self.increment_x();
            self.increment_y();
            return;
        }
        let step = if self.ctrl & 0x04 != 0 { 32 } else { 1 };
        self.v = self.v.wrapping_add(step) & 0x7FFF;
    }

    fn decay_open_bus(&mut self) {
        for bit in 0..8 {
            if self.total_dots.wrapping_sub(self.open_bus_refreshed[bit]) >= OPEN_BUS_DECAY_DOTS {
                self.open_bus &= !(1 << bit);
            }
        }
    }

    fn drive_open_bus(&mut self, value: u8, mask: u8) {
        self.open_bus = (self.open_bus & !mask) | (value & mask);
        for bit in 0..8 {
            if mask & (1 << bit) != 0 {
                self.open_bus_refreshed[bit] = self.total_dots;
            }
        }
    }

    pub(crate) fn initialize_legacy_accuracy_state(&mut self) {
        // v1 has no retention timers; begin a fresh interval for its latch.
        self.open_bus_refreshed = std::array::from_fn(|bit| {
            if self.open_bus & (1 << bit) != 0 {
                self.total_dots
            } else {
                0
            }
        });
    }

    pub(crate) fn save_accuracy_state(&self, out: &mut Vec<u8>) {
        use crate::state::Codec;
        self.open_bus_refreshed.encode(out);
    }

    pub(crate) fn load_accuracy_state(&mut self, input: &mut &[u8]) -> crate::state::Result<()> {
        use crate::state::Codec;
        self.open_bus_refreshed = Codec::decode(input)?;
        Ok(())
    }

    /// Convert a v1/v2 snapshot's static sprite records (screen X, unshifted
    /// patterns, selected at dot 257) into live counters and shifters.
    pub(crate) fn initialize_legacy_pipeline_state(&mut self) {
        self.pending_mask = self.mask;
        self.mask_delay = 0;
        self.v_delay = 0;
        self.data_ops = [DataOp::default(); 2];
        self.corrupt_arm = false;
        self.corrupt_pending = false;
        self.skip_glitch = false;
        self.pending_status = 0;
        self.oam2_full = false;
        self.oam2_sprite0 = false;
        self.oam_buffer = 0xff;
        let visible = self.scanline < 240 && (1..=256).contains(&self.dot);
        for (i, sprite) in self.sprites.iter_mut().enumerate() {
            self.sprite_loaded[i] = i < self.sprite_count;
            if i >= self.sprite_count {
                sprite.low = 0;
                sprite.high = 0;
            }
            if sprite.attributes & 0x40 != 0 {
                sprite.low = sprite.low.reverse_bits();
                sprite.high = sprite.high.reverse_bits();
            }
            if visible {
                let x = sprite.x as u16;
                let shifted = self.dot.saturating_sub(x).min(8);
                sprite.x = x.saturating_sub(self.dot) as u8;
                if shifted == 8 {
                    sprite.low = 0;
                    sprite.high = 0;
                } else {
                    sprite.low <<= shifted;
                    sprite.high <<= shifted;
                }
                self.counting[i] = sprite.x != 0;
            } else {
                self.counting[i] = self.dot >= 339 && sprite.x != 0;
            }
        }
        // The new renderer clocks the background after pixel output; the old
        // one clocked before the next pixel. Advance that one pending clock.
        if visible && self.rendering() {
            self.bg_low <<= 1;
            self.bg_high = (self.bg_high << 1) | 1;
            self.attr_low <<= 1;
            self.attr_high <<= 1;
            if self.dot & 7 == 0 {
                self.load_background();
            }
        }
    }

    /// v3 snapshot extension: everything the pipeline rewrite added.
    pub(crate) fn save_pipeline_state(&self, out: &mut Vec<u8>) {
        use crate::state::Codec;
        self.pending_mask.encode(out);
        self.mask_delay.encode(out);
        self.pending_v.encode(out);
        self.v_delay.encode(out);
        self.counting.encode(out);
        self.oam2.encode(out);
        self.oam2_addr.encode(out);
        self.oam2_full.encode(out);
        self.oam_buffer.encode(out);
        self.eval_in_range.encode(out);
        self.eval_done.encode(out);
        self.eval_bytes.encode(out);
        self.overflow_bytes.encode(out);
        self.oam2_sprite0.encode(out);
        self.bus_latch.encode(out);
        self.bus_high.encode(out);
        self.bus_data.encode(out);
        self.data_ops.encode(out);
        self.corrupt_arm.encode(out);
        self.corrupt_pending.encode(out);
        self.corrupt_row.encode(out);
        self.skip_glitch.encode(out);
        self.sprite_y.encode(out);
        self.sprite_tile.encode(out);
        self.sprite_loaded.encode(out);
        self.pending_status.encode(out);
    }

    pub(crate) fn load_pipeline_state(&mut self, input: &mut &[u8]) -> crate::state::Result<()> {
        use crate::state::{Codec, StateError};
        self.pending_mask = Codec::decode(input)?;
        self.mask_delay = Codec::decode(input)?;
        self.pending_v = Codec::decode(input)?;
        self.v_delay = Codec::decode(input)?;
        self.counting = Codec::decode(input)?;
        self.oam2 = Codec::decode(input)?;
        self.oam2_addr = Codec::decode(input)?;
        self.oam2_full = Codec::decode(input)?;
        self.oam_buffer = Codec::decode(input)?;
        self.eval_in_range = Codec::decode(input)?;
        self.eval_done = Codec::decode(input)?;
        self.eval_bytes = Codec::decode(input)?;
        self.overflow_bytes = Codec::decode(input)?;
        self.oam2_sprite0 = Codec::decode(input)?;
        self.bus_latch = Codec::decode(input)?;
        self.bus_high = Codec::decode(input)?;
        self.bus_data = Codec::decode(input)?;
        self.data_ops = Codec::decode(input)?;
        self.corrupt_arm = Codec::decode(input)?;
        self.corrupt_pending = Codec::decode(input)?;
        self.corrupt_row = Codec::decode(input)?;
        self.skip_glitch = Codec::decode(input)?;
        self.sprite_y = Codec::decode(input)?;
        self.sprite_tile = Codec::decode(input)?;
        self.sprite_loaded = Codec::decode(input)?;
        self.pending_status = Codec::decode(input)?;
        if self.mask_delay > WRITE_SETTLE_DOTS
            || self.pending_status & !0x60 != 0
            || self.v_delay > WRITE_SETTLE_DOTS
            || self.pending_v >= 0x8000
            || self.oam2_addr >= 32
            || self.eval_bytes > 3
            || (self.eval_in_range && self.eval_bytes == 0 && self.overflow_bytes == 0)
            || self.overflow_bytes > 3
            || self.bus_high >= 0x40
            || self.corrupt_row >= 32
            || self.data_ops.iter().any(|op| {
                // A pending access must still have its final dot ahead of it.
                let expires = if op.kind == 1 {
                    DATA_READ_DOT
                } else {
                    DATA_WRITE_DOT
                };
                op.kind > 3
                    || op.addr >= 0x4000
                    || op.age > DATA_READ_DOT
                    || (op.kind != 0 && op.age >= expires)
            })
        {
            return Err(StateError("Invalid PPU pipeline state"));
        }
        Ok(())
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

    /// Output one pixel, then clock the sprite units. X counters run whenever
    /// the line is visible; shifters only advance while rendering is enabled.
    fn render_pixel(&mut self) {
        let x = self.dot as usize - 1;
        let bit = 0x8000 >> self.x;
        let rendering = self.rendering();
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
        let show_sprites = self.mask & 0x10 != 0 && (x >= 8 || self.mask & 4 != 0);
        // On frames after the pre-render skip, the units emit their first pixel
        // at X=0 before counting begins (composite 2C02 behavior).
        let glitch = self.skip_glitch && self.scanline == 0 && self.dot == 1;
        self.skip_glitch = false;
        let mut chosen: Option<(u8, u8, bool)> = None;
        for i in 0..8 {
            if glitch {
                let pixel = (self.sprites[i].low >> 7) | ((self.sprites[i].high >> 7) << 1);
                if rendering {
                    self.sprites[i].low <<= 1;
                    self.sprites[i].high <<= 1;
                }
                if pixel != 0 && chosen.is_none() {
                    chosen = Some((pixel, self.sprites[i].attributes, self.sprites[i].zero));
                }
            }
            if self.counting[i] {
                self.sprites[i].x = self.sprites[i].x.saturating_sub(1);
                self.counting[i] = self.sprites[i].x != 0;
                continue;
            }
            let pixel = (self.sprites[i].low >> 7) | ((self.sprites[i].high >> 7) << 1);
            if rendering {
                self.sprites[i].low <<= 1;
                self.sprites[i].high <<= 1;
            }
            if pixel != 0 && chosen.is_none() {
                chosen = Some((pixel, self.sprites[i].attributes, self.sprites[i].zero));
            }
        }
        if show_sprites {
            if let Some((pixel, attributes, zero)) = chosen {
                if zero && background != 0 && x != 255 {
                    self.pending_status |= 0x40;
                }
                if background == 0 || attributes & 0x20 == 0 {
                    index = 0x10 | ((attributes & 3) * 4) | pixel;
                }
            }
        }
        let addr = if !rendering && self.v & 0x3f00 == 0x3f00 {
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

    /// An external bus read. Palette addresses reach the nametable RAM, since
    /// the palette is internal to the PPU.
    fn bus_read(&mut self, addr: u16, mapper: &mut dyn Mapper) -> u8 {
        match addr & 0x3fff {
            a @ 0..=0x1fff => mapper.ppu_read(a),
            a => self.vram[self.nametable_index(a & 0x2fff, mapper)],
        }
    }

    fn bus_write(&mut self, addr: u16, val: u8, mapper: &mut dyn Mapper) {
        match addr & 0x3fff {
            a @ 0..=0x1fff => mapper.ppu_write(a, val),
            a @ 0x2000..=0x3eff => {
                let i = self.nametable_index(a, mapper);
                self.vram[i] = val;
            }
            _ => {}
        }
    }

    /// Primary OAM as seen through the data bus: attribute bytes have no bits 2-4.
    fn oam_read(&self, addr: u8) -> u8 {
        self.oam[addr as usize] & if addr & 3 == 2 { 0xe3 } else { 0xff }
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
    region: Region::Ntsc,
    open_bus_refreshed: [0; 8],
    pending_mask: 0,
    mask_delay: 0,
    pending_v: 0,
    v_delay: 0,
    counting: [false; 8],
    oam2: [0xff; 32],
    oam2_addr: 0,
    oam2_full: false,
    oam_buffer: 0xff,
    eval_in_range: false,
    eval_done: false,
    eval_bytes: 0,
    overflow_bytes: 0,
    oam2_sprite0: false,
    bus_latch: 0,
    bus_high: 0,
    bus_data: 0,
    data_ops: [DataOp::default(); 2],
    corrupt_arm: false,
    corrupt_pending: false,
    corrupt_row: 0,
    skip_glitch: false,
    sprite_y: [0; 8],
    sprite_tile: [0; 8],
    sprite_loaded: [false; 8],
    pending_status: 0
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
