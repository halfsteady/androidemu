//! Mapper 69 — Sunsoft FME-7, and the 5B that is an FME-7 with a sound chip.
//!
//! A command/parameter pair rather than an address-decoded register file: the
//! game writes which register it means to $8000, then the value to $A000. That
//! buys sixteen registers out of two addresses — eight 1 KB CHR banks, four 8 KB
//! PRG windows, mirroring, and a 16-bit IRQ counter.
//!
//! The IRQ counter runs down on every CPU cycle, not on scanlines, which is what
//! makes it different from MMC3's: Batman's and Gremlins 2's raster effects want
//! a count of cycles, and there is no PPU address bus involved.
//!
//! **The 5B's audio expansion is not implemented.** Only Gimmick! uses it, and
//! only for extra channels — the game runs and its normal APU audio plays, it is
//! just missing three square waves. The registers are accepted and discarded so
//! the writes do not fall through to something that would misread them.

use super::super::{Header, Mirroring};
use super::Mapper;
use crate::state::{Codec, StateError};

#[derive(Clone)]
pub struct Fme7 {
    prg: Vec<u8>,
    chr: Vec<u8>,
    chr_ram: bool,
    ram: Vec<u8>,
    battery: bool,
    state: Registers,
}

#[derive(Clone)]
struct Registers {
    /// Which register the next write to $A000 lands in.
    command: u8,
    chr: [u8; 8],
    /// The $6000 window, then $8000, $A000 and $C000. $E000 is always the last bank.
    prg: [u8; 4],
    mirror: u8,
    irq_counter: u16,
    /// Bit 0 of the $D register: whether the counter runs at all.
    counting: bool,
    /// Bit 7: whether reaching zero is allowed to pull /IRQ.
    irq_enabled: bool,
    irq: bool,
}

crate::state::state_fields!(
    Registers,
    command,
    chr,
    prg,
    mirror,
    irq_counter,
    counting,
    irq_enabled,
    irq
);

impl Fme7 {
    pub fn new(h: &Header, prg: Vec<u8>, chr: Vec<u8>) -> Self {
        let chr_ram = chr.is_empty();
        let banks = (prg.len() / 8192).max(1) as u8;
        Self {
            prg,
            chr: if chr_ram { vec![0; h.chr_ram_size.max(8192)] } else { chr },
            chr_ram,
            ram: vec![0; h.prg_ram_size.max(8192)],
            battery: h.battery,
            state: Registers {
                command: 0,
                chr: [0; 8],
                // Power-on values are not defined by the board, but pointing the
                // windows at the last banks means the reset vector is reachable
                // before the game has configured anything.
                prg: [0, banks.saturating_sub(4), banks.saturating_sub(3), banks.saturating_sub(2)],
                mirror: u8::from(h.mirroring == Mirroring::Horizontal),
                irq_counter: 0,
                counting: false,
                irq_enabled: false,
                irq: false,
            },
        }
    }

    /// Whether the $6000 window is RAM (bit 7) rather than a PRG ROM bank.
    fn ram_window(&self) -> bool {
        self.state.prg[0] & 0x80 != 0
    }
    /// Whether that RAM answers at all (bit 6).
    fn ram_enabled(&self) -> bool {
        self.state.prg[0] & 0x40 != 0
    }

    fn prg_index(&self, addr: u16) -> usize {
        let window = (addr as usize - 0x8000) / 8192;
        let bank = match window {
            0..=2 => self.state.prg[window + 1] as usize & 0x3f,
            // $E000 is hard-wired to the last bank on this board.
            _ => (self.prg.len() / 8192).max(1) - 1,
        };
        (bank * 8192 + addr as usize % 8192) % self.prg.len()
    }

    fn chr_index(&self, addr: u16) -> usize {
        let slot = (addr as usize & 0x1fff) / 1024;
        let bank = self.state.chr[slot] as usize;
        (bank * 1024 + addr as usize % 1024) % self.chr.len()
    }
}

impl Mapper for Fme7 {
    fn clone_box(&self) -> Box<dyn Mapper> {
        Box::new(self.clone())
    }

    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x6000..=0x7fff => {
                if self.ram_window() {
                    // Disabled RAM leaves the bus open rather than reading zero.
                    if !self.ram_enabled() {
                        return None;
                    }
                    Some(self.ram[(addr as usize - 0x6000) % self.ram.len()])
                } else {
                    let bank = self.state.prg[0] as usize & 0x3f;
                    let i = (bank * 8192 + addr as usize % 8192) % self.prg.len();
                    Some(self.prg[i])
                }
            }
            0x8000..=0xffff => Some(self.prg[self.prg_index(addr)]),
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, val: u8) {
        if (0x6000..=0x7fff).contains(&addr) {
            if self.ram_window() && self.ram_enabled() {
                let i = (addr as usize - 0x6000) % self.ram.len();
                self.ram[i] = val;
            }
            return;
        }
        match addr & 0xe000 {
            0x8000 => self.state.command = val & 0x0f,
            0xa000 => {
                let s = &mut self.state;
                match s.command {
                    0..=7 => s.chr[s.command as usize] = val,
                    8..=0xb => s.prg[s.command as usize - 8] = val,
                    0xc => s.mirror = val & 3,
                    0xd => {
                        s.counting = val & 1 != 0;
                        s.irq_enabled = val & 0x80 != 0;
                        // Writing the control register acknowledges any pending
                        // interrupt, which is how the handler clears it.
                        s.irq = false;
                    }
                    0xe => s.irq_counter = (s.irq_counter & 0xff00) | val as u16,
                    _ => s.irq_counter = (s.irq_counter & 0x00ff) | ((val as u16) << 8),
                }
            }
            // The 5B's sound registers. Accepted and dropped: see the note above.
            _ => {}
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
        match self.state.mirror {
            0 => Mirroring::Vertical,
            1 => Mirroring::Horizontal,
            2 => Mirroring::SingleScreenLo,
            _ => Mirroring::SingleScreenHi,
        }
    }

    /// One CPU cycle of the counter. It wraps rather than stopping, and the
    /// interrupt fires on the wrap, so a game that never reloads gets one every
    /// 65536 cycles.
    fn tick(&mut self) {
        if !self.state.counting {
            return;
        }
        let (next, wrapped) = self.state.irq_counter.overflowing_sub(1);
        self.state.irq_counter = next;
        if wrapped && self.state.irq_enabled {
            self.state.irq = true;
        }
    }

    fn irq(&self) -> bool {
        self.state.irq
    }

    fn battery_ram(&self) -> Option<&[u8]> {
        if self.battery {
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
        if self.chr_ram {
            self.chr.encode(out);
        }
    }

    fn load_state(&mut self, input: &mut &[u8]) -> Result<(), StateError> {
        self.state = Registers::decode(input)?;
        let ram = Vec::<u8>::decode(input)?;
        if ram.len() != self.ram.len() {
            return Err(StateError("Invalid PRG RAM"));
        }
        self.ram = ram;
        if self.chr_ram {
            let chr = Vec::<u8>::decode(input)?;
            if chr.len() != self.chr.len() {
                return Err(StateError("Invalid CHR RAM"));
            }
            self.chr = chr;
        }
        Ok(())
    }
}
