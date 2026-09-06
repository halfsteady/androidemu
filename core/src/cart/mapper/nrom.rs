//! Mapper 0 - NROM. No banking at all: 16 or 32 KB of PRG, 8 KB of CHR.
//!
//! The 16 KB case mirrors PRG into both halves of $8000-$FFFF, which is why
//! Donkey Kong's reset vector at $FFFC reads correctly from a 16 KB image.

use super::super::{Header, Mirroring};
use super::Mapper;

#[derive(Clone)]
pub struct Nrom {
    prg: Vec<u8>,
    chr: Vec<u8>,
    prg_ram: Vec<u8>,
    chr_is_ram: bool,
    mirroring: Mirroring,
    battery: bool,
}

impl Nrom {
    pub fn new(header: &Header, prg: Vec<u8>, chr: Vec<u8>) -> Nrom {
        let chr_is_ram = chr.is_empty();
        let chr = if chr_is_ram {
            vec![0; header.chr_ram_size.max(8 * 1024)]
        } else {
            chr
        };
        Nrom {
            prg,
            chr,
            prg_ram: vec![0; header.prg_ram_size.max(8 * 1024)],
            chr_is_ram,
            mirroring: header.mirroring,
            battery: header.battery,
        }
    }
}

impl Mapper for Nrom {
    fn clone_box(&self) -> Box<dyn Mapper> {
        Box::new(self.clone())
    }
    fn save_state(&self, out: &mut Vec<u8>) {
        use crate::state::Codec;
        self.prg_ram.encode(out);
        if self.chr_is_ram {
            self.chr.encode(out);
        }
    }
    fn load_state(&mut self, input: &mut &[u8]) -> Result<(), crate::state::StateError> {
        use crate::state::{Codec, StateError};
        let ram = Vec::<u8>::decode(input)?;
        if ram.len() != self.prg_ram.len() {
            return Err(StateError("Invalid PRG RAM"));
        }
        self.prg_ram = ram;
        if self.chr_is_ram {
            let chr = Vec::<u8>::decode(input)?;
            if chr.len() != self.chr.len() {
                return Err(StateError("Invalid CHR RAM"));
            }
            self.chr = chr;
        }
        Ok(())
    }
    fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x6000..=0x7FFF => {
                let i = (addr as usize - 0x6000) % self.prg_ram.len();
                Some(self.prg_ram[i])
            }
            0x8000..=0xFFFF => {
                if self.prg.is_empty() {
                    return None;
                }
                let i = (addr as usize - 0x8000) % self.prg.len();
                Some(self.prg[i])
            }
            _ => None,
        }
    }

    fn cpu_write(&mut self, addr: u16, val: u8) {
        if let 0x6000..=0x7FFF = addr {
            let i = (addr as usize - 0x6000) % self.prg_ram.len();
            self.prg_ram[i] = val;
        }
        // Writes to $8000-$FFFF go nowhere on NROM: it is mask ROM.
    }

    fn ppu_read(&mut self, addr: u16) -> u8 {
        let i = (addr as usize) % self.chr.len();
        self.chr[i]
    }

    fn ppu_write(&mut self, addr: u16, val: u8) {
        if self.chr_is_ram {
            let i = (addr as usize) % self.chr.len();
            self.chr[i] = val;
        }
    }

    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }

    fn battery_ram(&self) -> Option<&[u8]> {
        if self.battery {
            Some(&self.prg_ram)
        } else {
            None
        }
    }

    fn load_battery_ram(&mut self, data: &[u8]) {
        let n = data.len().min(self.prg_ram.len());
        self.prg_ram[..n].copy_from_slice(&data[..n]);
    }
}
