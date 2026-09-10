//! Cartridge loading: iNES / NES 2.0 header parsing and mapper construction.
//!
//! A large fraction of ROMs in circulation carry wrong headers. Everything the
//! header claims is therefore treated as a *hint*: it is read, then repaired
//! against the file's own evidence, then corrected against the override database
//! — see [`fixup`] — and only then is the mapper built from it.

pub mod fixup;
pub mod mapper;

use fixup::Fixes;
use mapper::Mapper;

/// Nametable arrangement. The names are the ones the hardware docs use: `Horizontal`
/// means the nametables are arranged side by side, which mirrors *vertically*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mirroring {
    Horizontal,
    Vertical,
    SingleScreenLo,
    SingleScreenHi,
    FourScreen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    Ntsc,
    Pal,
    Dendy,
}

impl Region {
    pub fn cpu_hz(self) -> u32 {
        match self { Self::Ntsc => 1_789_773, Self::Pal => 1_662_607, Self::Dendy => 1_773_448 }
    }
    pub fn frame_rate(self) -> f64 {
        match self { Self::Ntsc => 60.0988, Self::Pal | Self::Dendy => 50.00698 }
    }
    pub fn scanlines(self) -> u16 { if self == Self::Ntsc { 262 } else { 312 } }
    pub fn vblank_scanline(self) -> u16 { if self == Self::Dendy { 291 } else { 241 } }
}

#[derive(Debug)]
pub enum CartError {
    TooShort,
    BadMagic,
    UnsupportedMapper(u16),
    InvalidHeader(&'static str),
}

impl core::fmt::Display for CartError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CartError::InvalidHeader(message) => f.write_str(message),
            CartError::TooShort => write!(f, "file is too short to be a ROM"),
            CartError::BadMagic => write!(f, "not an iNES file (missing NES\\x1A magic)"),
            CartError::UnsupportedMapper(n) => write!(f, "mapper {n} is not implemented yet"),
        }
    }
}

impl std::error::Error for CartError {}

/// Everything the header told us, kept separate from the live mapper state so it can
/// be logged, displayed, and overridden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub mapper: u16,
    pub submapper: u8,
    pub prg_rom_size: usize,
    pub chr_rom_size: usize,
    pub prg_ram_size: usize,
    pub chr_ram_size: usize,
    pub mirroring: Mirroring,
    pub battery: bool,
    pub trainer: bool,
    pub region: Region,
    pub nes2: bool,
    /// FNV-1a over the PRG+CHR payload, excluding the header.
    ///
    /// This is what savestates carry, so its meaning is fixed: changing it would
    /// reject every state already on the tablet. It covers exactly the bytes the
    /// final header describes, so a size correction does change it — which is
    /// right, because the state then belongs to a differently configured machine.
    pub hash: u64,
    /// FNV-1a over everything after the header and any trainer.
    ///
    /// The key the override database is matched on. Unlike [`Header::hash`] it
    /// does not depend on the sizes the header claims, which matters because a
    /// lying size is one of the things being corrected.
    pub identity: u64,
    /// What had to be corrected to get here.
    pub fixes: Fixes,
}

#[derive(Clone)]
pub struct Cartridge {
    pub header: Header,
    pub mapper: Box<dyn Mapper>,
}

// The mapper is a trait object with live state; only the header is worth printing.
impl core::fmt::Debug for Cartridge {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Cartridge")
            .field("header", &self.header)
            .finish_non_exhaustive()
    }
}

impl Cartridge {
    pub fn load(bytes: &[u8]) -> Result<Cartridge, CartError> {
        if bytes.len() < 16 {
            return Err(CartError::TooShort);
        }
        if &bytes[0..4] != b"NES\x1A" {
            return Err(CartError::BadMagic);
        }

        let flags6 = bytes[6];
        let flags7 = bytes[7];
        // NES 2.0 is signalled by bits 3-2 of flags7 being exactly binary 10.
        let nes2 = (flags7 & 0x0C) == 0x08;

        let mut prg_banks = bytes[4] as usize;
        let mut chr_banks = bytes[5] as usize;
        let mut prg_exponent = None;
        let mut chr_exponent = None;
        let mut mapper = ((flags7 & 0xF0) as u16) | ((flags6 >> 4) as u16);
        let mut submapper = 0u8;
        let mut prg_ram_size = 8 * 1024;
        let mut chr_ram_size = 0usize;
        let mut region = Region::Ntsc;

        if nes2 {
            let flags8 = bytes[8];
            mapper |= ((flags8 & 0x0F) as u16) << 8;
            submapper = flags8 >> 4;

            // Sizes gain 4 high bits each; the value 0xF switches to an exponent form.
            let hi = bytes[9];
            let prg_hi = (hi & 0x0F) as usize;
            let chr_hi = (hi >> 4) as usize;
            if prg_hi == 0x0F {
                prg_exponent = Some(exponent_size(bytes[4])?);
            } else {
                prg_banks |= prg_hi << 8;
            }
            if chr_hi == 0x0F {
                chr_exponent = Some(exponent_size(bytes[5])?);
            } else {
                chr_banks |= chr_hi << 8;
            }

            prg_ram_size = shift_size(bytes[10] & 0x0F) + shift_size(bytes[10] >> 4);
            chr_ram_size = shift_size(bytes[11] & 0x0F) + shift_size(bytes[11] >> 4);
            region = match bytes[12] & 0x03 {
                1 => Region::Pal,
                3 => Region::Dendy,
                _ => Region::Ntsc,
            };
            if prg_ram_size == 0 {
                prg_ram_size = 8 * 1024;
            }
        }
        // Archaic headers are detected and corrected in one place, by
        // `fixup::repair`, so that every correction gets reported.

        let prg_rom_size = prg_exponent.unwrap_or(prg_banks * 16 * 1024);
        let chr_rom_size = chr_exponent.unwrap_or(chr_banks * 8 * 1024);
        if !nes2 && bytes[9] & 1 != 0 {
            region = Region::Pal;
        }
        let trainer = flags6 & 0x04 != 0;
        let battery = flags6 & 0x02 != 0;

        let mirroring = if flags6 & 0x08 != 0 {
            Mirroring::FourScreen
        } else if flags6 & 0x01 != 0 {
            Mirroring::Vertical
        } else {
            Mirroring::Horizontal
        };

        let payload_start: usize = if trainer { 16 + 512 } else { 16 };
        if payload_start > bytes.len() {
            return Err(CartError::TooShort);
        }
        // Computed before anything is corrected, and over the whole remainder of
        // the file, so it identifies the ROM no matter what its header claims.
        let identity = fnv1a(&bytes[payload_start..]);

        let mut header = Header {
            mapper,
            submapper,
            prg_rom_size,
            chr_rom_size,
            prg_ram_size,
            chr_ram_size,
            mirroring,
            battery,
            trainer,
            region,
            nes2,
            hash: 0,
            identity,
            fixes: Fixes::NONE,
        };

        // What the file itself proves, then what only a database can know.
        let mut fixes = fixup::repair(&mut header, bytes, payload_start);
        fixes |= fixup::apply_table(&mut header);
        header.fixes = fixes;

        let payload_end = payload_start
            .checked_add(header.prg_rom_size)
            .and_then(|end| end.checked_add(header.chr_rom_size))
            .ok_or(CartError::InvalidHeader("ROM size overflow"))?;
        if payload_end > bytes.len() {
            return Err(CartError::TooShort);
        }
        if header.prg_rom_size == 0 {
            return Err(CartError::InvalidHeader("Game has no program ROM"));
        }

        let mut off = payload_start;
        let prg = bytes[off..off + header.prg_rom_size].to_vec();
        off += header.prg_rom_size;

        // A CHR ROM size of zero means the board carries CHR *RAM* instead.
        let chr = if header.chr_rom_size == 0 {
            Vec::new()
        } else {
            bytes[off..off + header.chr_rom_size].to_vec()
        };

        header.hash = fnv1a(&bytes[payload_start..payload_end]);

        let mapper = mapper::build(&header, prg, chr)?;
        Ok(Cartridge { header, mapper })
    }
}

/// NES 2.0 exponent-notation size: `2^exponent * (multiplier*2 + 1)` bytes.
fn exponent_size(b: u8) -> Result<usize, CartError> {
    let exponent = (b >> 2) as u32;
    let multiplier = (b & 0x03) as usize * 2 + 1;
    1usize
        .checked_shl(exponent)
        .and_then(|v| v.checked_mul(multiplier))
        .ok_or(CartError::InvalidHeader("ROM size overflow"))
}

/// NES 2.0 shift-notation RAM size: `64 << n` bytes, or zero.
fn shift_size(n: u8) -> usize {
    if n == 0 {
        0
    } else {
        64usize << n
    }
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}
