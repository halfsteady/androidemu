//! The boards that are a bank register and little else: MMC1, UxROM, CNROM,
//! MMC3, AxROM, GxROM, Color Dreams and Codemasters.
//!
//! One register file serves all of them, and each board reads the fields it needs
//! — `bank` means a different thing on UxROM than on GxROM. That is deliberate:
//! the alternative is eight structs that are each one `u8`. Boards that need
//! state of their own shape live in their own modules instead (see `latched` and
//! `fme7`), because bending this file around them would cost more than it saves.
//!
//! The field list is a savestate wire format. Adding to it invalidates every
//! state already saved for these boards, so a new board reuses a field that the
//! boards already here do not read.
use super::{Header, Mapper, Mirroring};
use crate::state::{Codec, StateError};

#[derive(Clone)]
pub struct Banked {
    prg: Vec<u8>,
    chr: Vec<u8>,
    chr_ram: bool,
    ram: Vec<u8>,
    kind: u16,
    fixed_mirroring: Mirroring,
    battery: bool,
    state: Registers,
}
#[derive(Clone)]
struct Registers {
    bank: u8,
    shift: u8,
    control: u8,
    chr0: u8,
    chr1: u8,
    prg: u8,
    select: u8,
    banks: [u8; 8],
    mirror: u8,
    ram_control: u8,
    irq_latch: u8,
    irq_count: u8,
    irq_reload: bool,
    irq_enabled: bool,
    irq: bool,
    a12: bool,
    low_since: u64,
    cycle: u64,
    last_write: u64,
}
crate::state::state_fields!(
    Registers,
    bank,
    shift,
    control,
    chr0,
    chr1,
    prg,
    select,
    banks,
    mirror,
    ram_control,
    irq_latch,
    irq_count,
    irq_reload,
    irq_enabled,
    irq,
    a12,
    low_since,
    cycle,
    last_write
);
impl Banked {
    pub fn new(h: &Header, prg: Vec<u8>, chr: Vec<u8>) -> Self {
        let chr_ram = chr.is_empty();
        Self {
            prg,
            chr: if chr_ram {
                vec![0; h.chr_ram_size.max(8192)]
            } else {
                chr
            },
            chr_ram,
            ram: vec![0; h.prg_ram_size.max(8192)],
            kind: h.mapper,
            fixed_mirroring: h.mirroring,
            battery: h.battery,
            state: Registers {
                bank: 0,
                shift: 0x10,
                // MMC1 powers up with both PRG bank-mode bits set. Every other
                // board here ignores this field, except Codemasters, which uses
                // it as its mirroring latch and must therefore start clear.
                control: if h.mapper == 1 { 0x0c } else { 0 },
                chr0: 0,
                chr1: 0,
                prg: 0,
                select: 0,
                banks: [0; 8],
                mirror: u8::from(h.mirroring == Mirroring::Horizontal),
                ram_control: 0x80,
                irq_latch: 0,
                irq_count: 0,
                irq_reload: false,
                irq_enabled: false,
                irq: false,
                a12: false,
                low_since: 0,
                cycle: 0,
                last_write: u64::MAX,
            },
        }
    }
    fn prg_index(&self, addr: u16) -> usize {
        let s = &self.state;
        let a = addr as usize - 0x8000;
        let n16 = self.prg.len() / 16384;
        let bank = match self.kind {
            1 => {
                let outer = if self.prg.len() > 262144 && self.chr_ram {
                    s.chr0 as usize & 0x10
                } else {
                    0
                };
                let selected = (s.prg as usize & 15) | outer;
                let b = match (s.control >> 2) & 3 {
                    0 | 1 => (selected & !1) + a / 16384,
                    2 => {
                        if a < 16384 {
                            outer
                        } else {
                            selected
                        }
                    }
                    _ => {
                        if a < 16384 {
                            selected
                        } else {
                            outer | (n16.min(16) - 1)
                        }
                    }
                };
                return (b * 16384 + a % 16384) % self.prg.len();
            }
            2 => {
                if a < 16384 {
                    s.bank as usize
                } else {
                    n16 - 1
                }
            }
            3 => a / 16384,
            7 => (s.bank as usize & 7) * 2 + a / 16384,
            66 => ((s.bank as usize >> 4) & 3) * 2 + a / 16384,
            // Color Dreams is GxROM with the nibbles the other way round: PRG in
            // the low bits, CHR in the high ones.
            11 => (s.bank as usize & 0x0f) * 2 + a / 16384,
            // Codemasters, same shape as UxROM.
            71 => {
                if a < 16384 {
                    s.bank as usize
                } else {
                    n16 - 1
                }
            }
            4 => {
                let n8 = self.prg.len() / 8192;
                let slot = a / 8192;
                let bank = match slot {
                    0 => {
                        if s.select & 0x40 != 0 {
                            n8 - 2
                        } else {
                            s.banks[6] as usize & 0x3f
                        }
                    }
                    1 => s.banks[7] as usize & 0x3f,
                    2 => {
                        if s.select & 0x40 != 0 {
                            s.banks[6] as usize & 0x3f
                        } else {
                            n8 - 2
                        }
                    }
                    _ => n8 - 1,
                };
                return (bank * 8192 + a % 8192) % self.prg.len();
            }
            _ => unreachable!(),
        };
        (bank * 16384 + a % 16384) % self.prg.len()
    }
    fn chr_index(&self, addr: u16) -> usize {
        let a = addr as usize;
        let s = &self.state;
        let i = match self.kind {
            1 => {
                let bank = if s.control & 0x10 == 0 {
                    (s.chr0 as usize & !1) + a / 4096
                } else if a < 4096 {
                    s.chr0 as usize
                } else {
                    s.chr1 as usize
                };
                bank * 4096 + a % 4096
            }
            3 => s.bank as usize * 8192 + a,
            66 => (s.bank as usize & 3) * 8192 + a,
            11 => (s.bank as usize >> 4) * 8192 + a,
            4 => {
                let slot = (a / 1024) ^ if s.select & 0x80 != 0 { 4 } else { 0 };
                let bank = match slot {
                    0..=1 => (s.banks[0] as usize & !1) + slot,
                    2..=3 => (s.banks[1] as usize & !1) + slot - 2,
                    _ => s.banks[slot - 2] as usize,
                };
                bank * 1024 + a % 1024
            }
            _ => a,
        };
        i % self.chr.len()
    }
    fn ram_enabled(&self) -> bool {
        match self.kind {
            1 => self.state.prg & 0x10 == 0,
            4 => self.state.ram_control & 0x80 != 0,
            _ => true,
        }
    }
}
impl Mapper for Banked {
    fn clone_box(&self) -> Box<dyn Mapper> {
        Box::new(self.clone())
    }
    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x6000..=0x7fff if self.ram_enabled() => {
                Some(self.ram[(addr as usize - 0x6000) % self.ram.len()])
            }
            0x8000..=0xffff => Some(self.prg[self.prg_index(addr)]),
            _ => None,
        }
    }
    fn cpu_write(&mut self, addr: u16, val: u8) {
        if (0x6000..=0x7fff).contains(&addr) && self.ram_enabled() {
            if self.kind != 4 || self.state.ram_control & 0x40 == 0 {
                let i = (addr as usize - 0x6000) % self.ram.len();
                self.ram[i] = val;
            }
            return;
        }
        if addr < 0x8000 {
            return;
        }
        let val = if self.kind == 66 { val & self.prg[self.prg_index(addr)] } else { val };
        let s = &mut self.state;
        match self.kind {
            1 => {
                // MMC1 ignores the second write of a CPU read-modify-write.
                if s.last_write != u64::MAX && s.cycle == s.last_write.wrapping_add(1) {
                    return;
                }
                s.last_write = s.cycle;
                if val & 0x80 != 0 {
                    s.shift = 0x10;
                    s.control |= 0x0c;
                    return;
                }
                let complete = s.shift & 1 != 0;
                s.shift = (s.shift >> 1) | ((val & 1) << 4);
                if complete {
                    match (addr >> 13) & 3 {
                        0 => s.control = s.shift,
                        1 => s.chr0 = s.shift,
                        2 => s.chr1 = s.shift,
                        _ => s.prg = s.shift,
                    }
                    s.shift = 0x10;
                }
            }
            4 => match addr & 0xe001 {
                0x8000 => s.select = val,
                0x8001 => s.banks[(s.select & 7) as usize] = val,
                0xa000 => s.mirror = val & 1,
                0xa001 => s.ram_control = val,
                0xc000 => s.irq_latch = val,
                0xc001 => s.irq_reload = true,
                0xe000 => {
                    s.irq_enabled = false;
                    s.irq = false;
                }
                0xe001 => s.irq_enabled = true,
                _ => unreachable!(),
            },
            // Codemasters BF9097 boards put a single-screen mirroring latch at
            // $9000, and the bank register responds to the rest of the range.
            // `control` is the latch: 0 until written, then 1 or 2.
            71 => {
                if (0x9000..=0x9fff).contains(&addr) {
                    s.control = if val & 0x10 != 0 { 2 } else { 1 };
                } else {
                    s.bank = val;
                }
            }
            _ => s.bank = val,
        }
    }
    fn ppu_read(&mut self, addr: u16) -> u8 {
        self.chr[self.chr_index(addr)]
    }
    fn ppu_write(&mut self, addr: u16, val: u8) {
        if self.chr_ram {
            let i = self.chr_index(addr);
            self.chr[i] = val;
        }
    }
    fn mirroring(&self) -> Mirroring {
        match self.kind {
            1 => match self.state.control & 3 {
                0 => Mirroring::SingleScreenLo,
                1 => Mirroring::SingleScreenHi,
                2 => Mirroring::Vertical,
                _ => Mirroring::Horizontal,
            },
            7 => {
                if self.state.bank & 0x10 == 0 {
                    Mirroring::SingleScreenLo
                } else {
                    Mirroring::SingleScreenHi
                }
            }
            71 => match self.state.control {
                1 => Mirroring::SingleScreenLo,
                2 => Mirroring::SingleScreenHi,
                // Boards without the latch never write it, and keep the header's.
                _ => self.fixed_mirroring,
            },
            4 if self.fixed_mirroring != Mirroring::FourScreen => {
                if self.state.mirror == 0 {
                    Mirroring::Vertical
                } else {
                    Mirroring::Horizontal
                }
            }
            _ => self.fixed_mirroring,
        }
    }
    fn tick(&mut self) {
        self.state.cycle = self.state.cycle.wrapping_add(1);
    }
    fn ppu_bus(&mut self, addr: u16, dot: u64) {
        if self.kind != 4 {
            return;
        }
        let s = &mut self.state;
        let high = addr & 0x1000 != 0;
        if s.a12 && !high {
            s.low_since = dot;
        }
        // Require more than the nine-dot nametable gap at a scanline boundary.
        // This dot-domain filter matches the common MMC3B/C scanline behavior;
        // revision-specific M2 edge phase remains outside this board model.
        if !s.a12 && high && dot.saturating_sub(s.low_since) >= 10 {
            if s.irq_count == 0 || s.irq_reload {
                s.irq_count = s.irq_latch;
                s.irq_reload = false;
            } else {
                s.irq_count -= 1;
            }
            if s.irq_count == 0 && s.irq_enabled {
                s.irq = true;
            }
        }
        s.a12 = high;
    }
    fn irq(&self) -> bool {
        self.state.irq
    }
    fn battery_ram(&self) -> Option<&[u8]> {
        self.battery.then_some(&self.ram)
    }
    fn load_battery_ram(&mut self, bytes: &[u8]) {
        let n = bytes.len().min(self.ram.len());
        self.ram[..n].copy_from_slice(&bytes[..n]);
    }
    fn save_state(&self, out: &mut Vec<u8>) {
        self.state.encode(out);
        self.ram.encode(out);
        if self.chr_ram {
            self.chr.encode(out);
        }
    }
    fn load_state(&mut self, input: &mut &[u8]) -> Result<(), StateError> {
        self.state = Registers::decode(input)?;
        let ram = Vec::<u8>::decode(input)?;
        if ram.len() != self.ram.len() {
            return Err(StateError("Invalid mapper RAM"));
        }
        self.ram = ram;
        if self.chr_ram {
            let chr = Vec::<u8>::decode(input)?;
            if chr.len() != self.chr.len() {
                return Err(StateError("Invalid mapper CHR"));
            }
            self.chr = chr;
        }
        Ok(())
    }
}
