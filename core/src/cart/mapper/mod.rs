//! Mapper (cartridge board) implementations.
//!
//! Tier 1 - mappers 0/1/2/3/4/7 - is roughly 90% of the commercial library and is
//! what Phase 1 targets. See PLAN.md §1.

mod nrom;

use super::{CartError, Header, Mirroring};

/// A cartridge board. The CPU and PPU see the cartridge only through this trait.
///
/// `cpu_read` and `cpu_write` cover $4020-$FFFF; `ppu_read` and `ppu_write` cover
/// $0000-$1FFF (pattern tables). Nametable mirroring is reported rather than mapped,
/// because most boards only ever select one of the fixed arrangements.
pub trait Mapper: Send {
    fn cpu_read(&mut self, addr: u16) -> Option<u8>;
    fn cpu_write(&mut self, addr: u16, val: u8);
    fn ppu_read(&mut self, addr: u16) -> u8;
    fn ppu_write(&mut self, addr: u16, val: u8);

    fn mirroring(&self) -> Mirroring;

    /// Called once per CPU cycle. Boards with a scanline counter (MMC3) or an audio
    /// expansion (VRC6, N163) need it; most do not.
    fn tick(&mut self) {}

    /// True while the board is asserting /IRQ.
    fn irq(&self) -> bool {
        false
    }

    /// Battery-backed save RAM, if the board has any.
    fn battery_ram(&self) -> Option<&[u8]> {
        None
    }
    fn load_battery_ram(&mut self, _data: &[u8]) {}
}

pub fn build(header: &Header, prg: Vec<u8>, chr: Vec<u8>) -> Result<Box<dyn Mapper>, CartError> {
    match header.mapper {
        0 => Ok(Box::new(nrom::Nrom::new(header, prg, chr))),
        n => Err(CartError::UnsupportedMapper(n)),
    }
}
