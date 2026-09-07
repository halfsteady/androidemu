//! Making a wrong header right.
//!
//! A large fraction of the ROMs in circulation carry headers that disagree with
//! the board they came off. There are two different problems in there, and they
//! want two different answers:
//!
//! 1. **Damage that the file itself proves.** A ripping tool wrote its name over
//!    bytes 7-15; the claimed sizes run past the end of the file. These need no
//!    outside knowledge — the file contradicts itself, and [`repair`] fixes it.
//!
//! 2. **Claims that are merely wrong.** The mapper number, the mirroring, whether
//!    there is a battery. Nothing in the file says so; it takes a database keyed
//!    by the ROM's identity, which is [`TABLE`].
//!
//! The table ships empty on purpose. Populating it with hashes nobody has checked
//! would quietly mis-configure real games, which is worse than leaving the header
//! alone — so `scripts/build-header-db.py` generates it from ROMs you actually
//! have, matched against a published board database. See PLAN.md §1.

use super::{Header, Mirroring, Region};

/// What was changed, so it can be shown rather than done silently behind the
/// player's back. A header that needed fixing is worth knowing about: it is
/// usually the reason a game looks wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fixes(u32);

impl Fixes {
    pub const NONE: Fixes = Fixes(0);
    /// Bytes 12-15 held junk, so the high mapper bits in byte 7 were not trustworthy.
    pub const ARCHAIC_MAPPER: Fixes = Fixes(1 << 0);
    /// The junk was the specific calling card of a long-dead ripping tool.
    pub const DISK_DUDE: Fixes = Fixes(1 << 1);
    /// The claimed CHR ran past the end of the file.
    pub const CHR_TRUNCATED: Fixes = Fixes(1 << 2);
    /// The claimed PRG ran past the end of the file.
    pub const PRG_TRUNCATED: Fixes = Fixes(1 << 3);
    /// The claimed PRG is impossible for the claimed mapper.
    pub const IMPLAUSIBLE_MAPPER: Fixes = Fixes(1 << 4);
    /// Byte 9's PAL flag was junk from the same dead tool.
    pub const ARCHAIC_REGION: Fixes = Fixes(1 << 9);
    /// The override database replaced the mapper or submapper.
    pub const TABLE_MAPPER: Fixes = Fixes(1 << 5);
    /// The override database replaced the mirroring.
    pub const TABLE_MIRRORING: Fixes = Fixes(1 << 6);
    /// The override database replaced the battery or RAM sizes.
    pub const TABLE_RAM: Fixes = Fixes(1 << 7);
    /// The override database replaced the region.
    pub const TABLE_REGION: Fixes = Fixes(1 << 8);

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub fn has(self, other: Fixes) -> bool {
        self.0 & other.0 == other.0 && !other.is_empty()
    }
    /// Whether anything came out of the override table, as opposed to the file.
    pub fn from_table(self) -> bool {
        self.has(Fixes::TABLE_MAPPER)
            || self.has(Fixes::TABLE_MIRRORING)
            || self.has(Fixes::TABLE_RAM)
            || self.has(Fixes::TABLE_REGION)
    }
    /// Plain descriptions, for the runner's `info` and the app's problem log.
    pub fn labels(self) -> impl Iterator<Item = &'static str> {
        const NAMES: [(Fixes, &str); 10] = [
            (Fixes::ARCHAIC_MAPPER, "ignored the high mapper bits: bytes 7-15 held junk"),
            (Fixes::ARCHAIC_REGION, "ignored the PAL flag in the same junk, and read it as NTSC"),
            (Fixes::DISK_DUDE, "header carried the DiskDude! ripper's calling card"),
            (Fixes::CHR_TRUNCATED, "trimmed the claimed CHR to what the file actually holds"),
            (Fixes::PRG_TRUNCATED, "trimmed the claimed PRG to what the file actually holds"),
            (Fixes::IMPLAUSIBLE_MAPPER, "claimed mapper cannot address this much PRG"),
            (Fixes::TABLE_MAPPER, "override database set the mapper"),
            (Fixes::TABLE_MIRRORING, "override database set the mirroring"),
            (Fixes::TABLE_RAM, "override database set the battery or RAM size"),
            (Fixes::TABLE_REGION, "override database set the region"),
        ];
        NAMES.into_iter().filter(move |(bit, _)| self.has(*bit)).map(|(_, name)| name)
    }
}

impl core::ops::BitOr for Fixes {
    type Output = Fixes;
    fn bitor(self, rhs: Fixes) -> Fixes {
        Fixes(self.0 | rhs.0)
    }
}

impl core::ops::BitOrAssign for Fixes {
    fn bitor_assign(&mut self, rhs: Fixes) {
        self.0 |= rhs.0;
    }
}

/// One game's corrections. Every field is optional: an entry says only what the
/// header got wrong, so a mapper fix does not also assert a mirroring.
#[derive(Debug, Clone, Copy)]
pub struct Override {
    /// FNV-1a over everything after the 16-byte header and any trainer. Chosen
    /// over the payload hash because it does not depend on the sizes the header
    /// claims — and a lying size is one of the things being corrected.
    pub identity: u64,
    /// Only for diagnostics; the table is matched on [`Override::identity`].
    pub name: &'static str,
    pub mapper: Option<u16>,
    pub submapper: Option<u8>,
    pub mirroring: Option<Mirroring>,
    pub battery: Option<bool>,
    pub region: Option<Region>,
    pub prg_ram_size: Option<usize>,
    pub chr_ram_size: Option<usize>,
}

impl Override {
    /// A blank entry to build from, so a table row names only what it corrects.
    pub const fn for_rom(identity: u64, name: &'static str) -> Override {
        Override {
            identity,
            name,
            mapper: None,
            submapper: None,
            mirroring: None,
            battery: None,
            region: None,
            prg_ram_size: None,
            chr_ram_size: None,
        }
    }
}

include!("overrides.rs");

/// The table entry for a ROM, or `None` when the header is on its own.
///
/// Binary search, so the generated table must stay sorted by identity — which
/// [`table_is_sorted`] holds the generator to.
pub fn lookup(identity: u64) -> Option<&'static Override> {
    TABLE
        .binary_search_by(|entry| entry.identity.cmp(&identity))
        .ok()
        .map(|at| &TABLE[at])
}

/// Fixes the header can be held to from the file alone, before any database.
///
/// `payload` is everything after the 16-byte header and any trainer — the bytes
/// the sizes are supposed to describe.
pub fn repair(header: &mut Header, raw: &[u8], payload: usize) -> Fixes {
    let mut fixes = Fixes::NONE;

    // The NesDev rule for telling the three header generations apart: NES 2.0 if
    // byte 7's bits 3-2 are binary 10; plain iNES 1.0 if they are 00 *and* bytes
    // 12-15 are clear; otherwise archaic, meaning bytes 7-15 belong to some tool
    // that is not the format and none of them can be believed.
    //
    // This is the only place that decision is made, so a header cannot be quietly
    // repaired somewhere else without it being reported here.
    let archaic = !header.nes2
        && raw.len() >= 16
        && (raw[7] & 0x0C != 0 || raw[12..16].iter().any(|&b| b != 0));
    if archaic {
        // Byte 7 contributes bits 4-7 of the mapper, so junk there does not read
        // as a neighbouring mapper — it reads as one 16 higher. A DiskDude'd NROM
        // game claims mapper 64 and fails to load at all.
        if header.mapper & 0xF0 != 0 {
            header.mapper &= 0x0F;
            fixes |= Fixes::ARCHAIC_MAPPER;
        }
        // Byte 9 bit 0 is the iNES 1.0 PAL flag, and junk sets it about half the
        // time. Left alone, an NTSC game runs its whole soundtrack at 50 Hz.
        if header.region != Region::Ntsc {
            header.region = Region::Ntsc;
            fixes |= Fixes::ARCHAIC_REGION;
        }
        // The famous one. Named separately because seeing it in a log tells you
        // exactly which ripping tool the file has been through.
        if &raw[7..16] == b"DiskDude!" {
            fixes |= Fixes::DISK_DUDE;
        }
    }

    // Sizes that run past the end of the file, which is the other thing the file
    // proves on its own. Two cases hide in here and only one is a header lie:
    //
    //   * A whole number of banks is missing. The header overstated the board —
    //     very often a CHR-RAM game whose header claims one CHR bank — and
    //     trimming to what is there is exactly right.
    //   * Part of a bank is missing. The file is damaged, not mislabelled.
    //     Trimming would throw away real data and hand back a game that renders
    //     garbage, which is worse than saying so: that stays an error.
    const PRG_BANK: usize = 16 * 1024;
    const CHR_BANK: usize = 8 * 1024;
    let available = raw.len().saturating_sub(payload);
    if header.prg_rom_size > available {
        if available >= PRG_BANK && available % PRG_BANK == 0 {
            header.prg_rom_size = available;
            header.chr_rom_size = 0;
            fixes |= Fixes::PRG_TRUNCATED;
        }
        // Otherwise left alone, and `load` rejects it as too short.
    } else if header.prg_rom_size + header.chr_rom_size > available {
        let spare = available - header.prg_rom_size;
        if spare % CHR_BANK == 0 {
            header.chr_rom_size = spare;
            fixes |= Fixes::CHR_TRUNCATED;
        }
    }
    if header.chr_rom_size == 0 && header.chr_ram_size == 0 {
        header.chr_ram_size = 8 * 1024;
    }

    // NROM has no banking hardware, so it cannot reach past 32 KB of PRG or 8 KB
    // of CHR. Reported rather than guessed at: which mapper it *should* be is
    // exactly the per-game knowledge the table exists to hold.
    if header.mapper == 0 && (header.prg_rom_size > 32 * 1024 || header.chr_rom_size > 8 * 1024) {
        fixes |= Fixes::IMPLAUSIBLE_MAPPER;
    }

    fixes
}

/// Applies the override table on top of the file's own evidence.
pub fn apply_table(header: &mut Header) -> Fixes {
    let mut fixes = Fixes::NONE;
    let Some(entry) = lookup(header.identity) else {
        return fixes;
    };
    if let Some(mapper) = entry.mapper {
        if mapper != header.mapper {
            header.mapper = mapper;
            fixes |= Fixes::TABLE_MAPPER;
        }
    }
    if let Some(submapper) = entry.submapper {
        if submapper != header.submapper {
            header.submapper = submapper;
            fixes |= Fixes::TABLE_MAPPER;
        }
    }
    if let Some(mirroring) = entry.mirroring {
        if mirroring != header.mirroring {
            header.mirroring = mirroring;
            fixes |= Fixes::TABLE_MIRRORING;
        }
    }
    if let Some(battery) = entry.battery {
        if battery != header.battery {
            header.battery = battery;
            fixes |= Fixes::TABLE_RAM;
        }
    }
    if let Some(size) = entry.prg_ram_size {
        if size != header.prg_ram_size {
            header.prg_ram_size = size;
            fixes |= Fixes::TABLE_RAM;
        }
    }
    if let Some(size) = entry.chr_ram_size {
        if size != header.chr_ram_size {
            header.chr_ram_size = size;
            fixes |= Fixes::TABLE_RAM;
        }
    }
    if let Some(region) = entry.region {
        if region != header.region {
            header.region = region;
            fixes |= Fixes::TABLE_REGION;
        }
    }
    fixes
}

/// The generator emits a sorted table and `lookup` binary-searches it; this is
/// what stops the two from ever disagreeing quietly.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_and_unique() {
        for pair in TABLE.windows(2) {
            assert!(
                pair[0].identity < pair[1].identity,
                "override table must be sorted and free of duplicates: {:016x} then {:016x}",
                pair[0].identity,
                pair[1].identity,
            );
        }
    }

    #[test]
    fn lookup_finds_nothing_in_an_empty_table() {
        // Meaningful while the shipped table is empty: an absent entry has to be
        // absent rather than a panic or a false match on the first row.
        assert!(TABLE.iter().all(|entry| lookup(entry.identity).is_some()));
        assert!(lookup(0xdead_beef_dead_beef).is_none() || !TABLE.is_empty());
    }
}
