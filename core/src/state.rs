//! Versioned, bounded, little-endian machine snapshots. Loading is transactional.
use crate::Nes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateError(pub &'static str);
impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for StateError {}
pub(crate) type Result<T> = std::result::Result<T, StateError>;

pub(crate) trait Codec: Sized {
    fn encode(&self, out: &mut Vec<u8>);
    fn decode(input: &mut &[u8]) -> Result<Self>;
}
macro_rules! number {
    ($($t:ty),*) => {$ (impl Codec for $t {
        fn encode(&self, out: &mut Vec<u8>) { out.extend_from_slice(&self.to_le_bytes()); }
        fn decode(input: &mut &[u8]) -> Result<Self> {
            let n = std::mem::size_of::<Self>();
            if input.len() < n { return Err(StateError("Truncated save state")); }
            let (bytes, rest) = input.split_at(n); *input = rest;
            Ok(Self::from_le_bytes(bytes.try_into().unwrap()))
        }
    })*};
}
number!(u8, u16, u32, u64, i16, f32, f64);
impl Codec for bool {
    fn encode(&self, out: &mut Vec<u8>) {
        (*self as u8).encode(out);
    }
    fn decode(input: &mut &[u8]) -> Result<Self> {
        match u8::decode(input)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(StateError("Invalid boolean")),
        }
    }
}
impl Codec for usize {
    fn encode(&self, out: &mut Vec<u8>) {
        (*self as u64).encode(out);
    }
    fn decode(input: &mut &[u8]) -> Result<Self> {
        usize::try_from(u64::decode(input)?).map_err(|_| StateError("Invalid size"))
    }
}
impl<T: Codec, const N: usize> Codec for [T; N] {
    fn encode(&self, out: &mut Vec<u8>) {
        for v in self {
            v.encode(out);
        }
    }
    fn decode(input: &mut &[u8]) -> Result<Self> {
        (0..N)
            .map(|_| T::decode(input))
            .collect::<Result<Vec<_>>>()?
            .try_into()
            .map_err(|_| StateError("Invalid array"))
    }
}
impl<T: Codec> Codec for Vec<T> {
    fn encode(&self, out: &mut Vec<u8>) {
        self.len().encode(out);
        for v in self {
            v.encode(out);
        }
    }
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let n = usize::decode(input)?;
        if n > 1_048_576 || n > input.len() {
            return Err(StateError("Invalid buffer length"));
        }
        (0..n).map(|_| T::decode(input)).collect()
    }
}
macro_rules! state_fields {
    ($t:ty, $($field:ident),+ $(,)? $(; $($default_field:ident: $default:expr),+ )?) => {
        impl crate::state::Codec for $t {
            fn encode(&self, out: &mut Vec<u8>) { $(crate::state::Codec::encode(&self.$field, out);)+ }
            fn decode(input: &mut &[u8]) -> crate::state::Result<Self> {
                Ok(Self { $($field: crate::state::Codec::decode(input)?,)+ $($($default_field: $default,)+)? })
            }
        }
    };
}
pub(crate) use state_fields;
fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    })
}
fn board_signature(nes: &Nes) -> u64 {
    let h = &nes.bus.cart.header;
    let mut bytes = Vec::new();
    h.mapper.encode(&mut bytes);
    h.submapper.encode(&mut bytes);
    (h.mirroring as u8).encode(&mut bytes);
    (h.region as u8).encode(&mut bytes);
    h.prg_rom_size.encode(&mut bytes);
    h.chr_rom_size.encode(&mut bytes);
    h.prg_ram_size.encode(&mut bytes);
    h.chr_ram_size.encode(&mut bytes);
    h.battery.encode(&mut bytes);
    checksum(&bytes)
}
impl Nes {
    pub fn save_state(&self) -> Vec<u8> {
        // Retain the released v1 body and v2 extension byte-for-byte in order.
        // v3 appends the DMA and rendering pipeline state. Older readers' paths
        // initialize fields absent from those formats with compatible defaults.
        let mut out = b"ANES\x03\x00\x00\x00".to_vec();
        self.bus.cart.header.hash.encode(&mut out);
        self.bus.cart.header.mapper.encode(&mut out);
        board_signature(self).encode(&mut out);
        self.cpu.encode(&mut out);
        self.bus.save_state(&mut out);
        self.bus.cart.mapper.save_state(&mut out);
        self.bus.save_accuracy_state(&mut out);
        self.bus.save_pipeline_state(&mut out);
        checksum(&out).encode(&mut out);
        out
    }
    /// Rejects wrong ROMs, unknown versions and corrupt data without changing the machine.
    pub fn load_state(&mut self, data: &[u8]) -> Result<()> {
        if data.len() < 26 || data.len() > 4 * 1024 * 1024 {
            return Err(StateError("Invalid save state size"));
        }
        let version = data[4];
        if &data[..4] != b"ANES" || !matches!(version, 1..=3) || data[5..8] != [0; 3] {
            return Err(StateError("Unsupported save state version"));
        }
        let end = data.len() - 8;
        if checksum(&data[..end]) != u64::from_le_bytes(data[end..].try_into().unwrap()) {
            return Err(StateError("Save state checksum mismatch"));
        }
        let mut input = &data[8..end];
        if u64::decode(&mut input)? != self.bus.cart.header.hash
            || u16::decode(&mut input)? != self.bus.cart.header.mapper
        {
            return Err(StateError("Save state belongs to a different game"));
        }
        if u64::decode(&mut input)? != board_signature(self) {
            return Err(StateError(
                "Save state uses a different cartridge configuration",
            ));
        }
        let mut candidate = self.clone();
        candidate.cpu = crate::Cpu::decode(&mut input)?;
        candidate.bus.load_state(&mut input)?;
        candidate.bus.cart.mapper.load_state(&mut input)?;
        if version >= 2 {
            candidate.bus.load_accuracy_state(&mut input)?;
        }
        if version >= 3 {
            candidate.bus.load_pipeline_state(&mut input)?;
        }
        if !input.is_empty() {
            return Err(StateError("Unexpected save state data"));
        }
        *self = candidate;
        Ok(())
    }
    pub fn battery_ram(&self) -> Option<&[u8]> {
        self.bus.cart.mapper.battery_ram()
    }
    pub fn load_battery_ram(&mut self, bytes: &[u8]) -> Result<()> {
        let expected = self
            .battery_ram()
            .ok_or(StateError("This game has no battery save"))?
            .len();
        if bytes.len() != expected {
            return Err(StateError("Battery save has the wrong size"));
        }
        self.bus.cart.mapper.load_battery_ram(bytes);
        Ok(())
    }
}
