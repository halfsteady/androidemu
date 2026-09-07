//! MMC2 (mapper 9, PxROM) and MMC4 (mapper 10, FxROM).
//!
//! These two are here together because they share the one thing that makes them
//! unusual: the CHR bank is not chosen by a register the CPU writes, it is chosen
//! by *what the PPU just looked at*. Each 4 KB pattern window has two bank
//! registers, and a fetch of tile $FD or $FE in that window latches which of the
//! two is live from the next fetch onwards.
//!
//! It exists because Punch-Out!! wanted more animation frames than its CHR could
//! address, so the board watches for a tile the game never draws and uses it as a
//! signal. Getting the ordering wrong — latching before the fetch that triggered
//! it returns — swaps a frame early and makes Little Mac flicker.
//!
//! The two boards differ only in PRG layout: MMC2 switches one 8 KB window and
//! fixes the last three, while MMC4 switches 16 KB and fixes the last 16 KB, and
//! has battery-backed RAM at $6000.

use super::super::{Header, Mirroring};
use super::Mapper;
use crate::state::{Codec, StateError};

#[derive(Clone)]
pub struct Latched {
    prg: Vec<u8>,
    chr: Vec<u8>,
    ram: Vec<u8>,
    /// True for MMC4: 16 KB PRG windows and working RAM.
    wide: bool,
    battery: bool,
    state: Registers,
}

#[derive(Clone)]
struct Registers {
    prg: u8,
    /// CHR banks for $0000-$0FFF, selected by `latch_lo`, then for $1000-$1FFF.
    chr: [u8; 4],
    /// Which of each window's two banks is live: 0 for the $FD one, 1 for $FE.
    latch_lo: u8,
    latch_hi: u8,
    mirror: u8,
}

crate::state::state_fields!(Registers, prg, chr, latch_lo, latch_hi, mirror);

impl Latched {
    pub fn new(h: &Header, prg: Vec<u8>, chr: Vec<u8>) -> Self {
        Self {
            prg,
            chr: if chr.is_empty() { vec![0; h.chr_ram_size.max(8192)] } else { chr },
            ram: vec![0; h.prg_ram_size.max(8192)],
            wide: h.mapper == 10,
            battery: h.battery,
            state: Registers {
                prg: 0,
                chr: [0; 4],
                // Both windows come up on their $FE bank, which is what the board
                // does at power-on before any tile has been fetched.
                latch_lo: 1,
                latch_hi: 1,
                mirror: u8::from(h.mirroring == Mirroring::Horizontal),
            },
        }
    }

    fn prg_index(&self, addr: u16) -> usize {
        let a = addr as usize - 0x8000;
        if self.wide {
            // 16 KB switchable, then the last 16 KB fixed.
            let n16 = (self.prg.len() / 16384).max(1);
            let bank = if a < 16384 { self.state.prg as usize & 0x0f } else { n16 - 1 };
            (bank * 16384 + a % 16384) % self.prg.len()
        } else {
            // 8 KB switchable, then the last three 8 KB banks fixed.
            let n8 = (self.prg.len() / 8192).max(1);
            let bank = match a / 8192 {
                0 => self.state.prg as usize & 0x0f,
                1 => n8.saturating_sub(3),
                2 => n8.saturating_sub(2),
                _ => n8 - 1,
            };
            (bank * 8192 + a % 8192) % self.prg.len()
        }
    }

    fn chr_index(&self, addr: u16) -> usize {
        let a = addr as usize & 0x1fff;
        let bank = if a < 4096 {
            self.state.chr[self.state.latch_lo as usize & 1]
        } else {
            self.state.chr[2 + (self.state.latch_hi as usize & 1)]
        };
        (bank as usize * 4096 + a % 4096) % self.chr.len()
    }

    /// The latch moves *after* the fetch that triggered it, so the tile which
    /// does the switching is itself drawn from the outgoing bank.
    ///
    /// MMC2 reacts to four exact addresses; MMC4 reacts to the eight-byte range
    /// around each, which matters because MMC4 games fetch the attribute byte of
    /// the same tile and would otherwise not latch at all.
    fn latch(&mut self, addr: u16) {
        let addr = addr & 0x3fff;
        let hit = |base: u16| {
            if self.wide {
                (base..base + 8).contains(&addr)
            } else {
                addr == base
            }
        };
        if hit(0x0fd8) {
            self.state.latch_lo = 0;
        } else if hit(0x0fe8) {
            self.state.latch_lo = 1;
        } else if hit(0x1fd8) {
            self.state.latch_hi = 0;
        } else if hit(0x1fe8) {
            self.state.latch_hi = 1;
        }
    }
}

impl Mapper for Latched {
    fn clone_box(&self) -> Box<dyn Mapper> {
        Box::new(self.clone())
    }

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            // Only MMC4 has RAM here; MMC2 leaves the bus open.
            0x6000..=0x7fff if self.wide => Some(self.ram[(addr as usize - 0x6000) % self.ram.len()]),
            0x8000..=0xffff => Some(self.prg[self.prg_index(addr)]),
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, val: u8) {
        if (0x6000..=0x7fff).contains(&addr) {
            if self.wide {
                let i = (addr as usize - 0x6000) % self.ram.len();
                self.ram[i] = val;
            }
            return;
        }
        // Both boards decode only the top nibble, so $A000 and $AFFF are one
        // register. Writes below $A000 do nothing on either.
        let s = &mut self.state;
        match addr & 0xf000 {
            0xa000 => s.prg = val & 0x0f,
            0xb000 => s.chr[0] = val & 0x1f,
            0xc000 => s.chr[1] = val & 0x1f,
            0xd000 => s.chr[2] = val & 0x1f,
            0xe000 => s.chr[3] = val & 0x1f,
            0xf000 => s.mirror = val & 1,
            _ => {}
        }
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        let value = self.chr[self.chr_index(addr)];
        self.latch(addr);
        value
    }

    fn ppu_write(&mut self, addr: u16, _val: u8) {
        // Both boards carry CHR ROM, so a write goes nowhere — but it still
        // drives the address bus, and the latch is watching it.
        self.latch(addr);
    }

    fn mirroring(&self) -> Mirroring {
        if self.state.mirror == 0 {
            Mirroring::Vertical
        } else {
            Mirroring::Horizontal
        }
    }

    fn battery_ram(&self) -> Option<&[u8]> {
        if self.battery && self.wide {
            Some(&self.ram)
        } else {
            None
        }
    }

    fn load_battery_ram(&mut self, data: &[u8]) {
        let n = data.len().min(self.ram.len());
        self.ram[..n].copy_from_slice(&data[..n]);
    }

    fn save_state(&self, out: &mut Vec<u8>) {
        self.state.encode(out);
        self.ram.encode(out);
    }

    fn load_state(&mut self, input: &mut &[u8]) -> Result<(), StateError> {
        self.state = Registers::decode(input)?;
        let ram = Vec::<u8>::decode(input)?;
        if ram.len() != self.ram.len() {
            return Err(StateError("Invalid PRG RAM"));
        }
        self.ram = ram;
        Ok(())
    }
}
