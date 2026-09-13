//! Session-owned undo history. Core hardware and serialized formats stay opaque.
use std::collections::VecDeque;

struct Entry {
    bytes: Box<[u8]>,
    previous_len: usize,
}

pub struct Rewind {
    current: Box<[u8]>,
    entries: VecDeque<Entry>,
    budget: usize,
    payload: usize,
    largest: usize,
    anchored: bool,
}

impl Rewind {
    pub fn new(budget: usize) -> Self {
        Self {
            current: Box::default(),
            entries: VecDeque::new(),
            budget,
            payload: 0,
            largest: 0,
            anchored: false,
        }
    }
    pub fn clear(&mut self) {
        self.current = Box::default();
        self.entries = VecDeque::new();
        self.payload = 0;
        self.largest = 0;
        self.anchored = false;
    }
    pub fn depth(&self) -> usize {
        self.entries.len()
    }
    /// Conservatively includes anchors, restore scratch, caller snapshot and
    /// worst-case encoding scratch. Allocator-internal bookkeeping is excluded.
    pub fn bytes(&self) -> usize {
        self.payload
            + self.largest.saturating_mul(16)
            + 3 * self.entries.capacity() * std::mem::size_of::<Entry>()
    }
    pub fn push(&mut self, state: &[u8]) {
        let largest = self.largest.max(state.len());
        if largest.saturating_mul(16) > self.budget {
            self.clear();
            return;
        }
        self.largest = largest;
        // Reserve worst-case delta output and deque growth before allocating.
        let extra = largest.saturating_mul(3) + 12 * std::mem::size_of::<Entry>();
        while self.bytes().saturating_add(extra) > self.budget {
            if let Some(entry) = self.entries.pop_front() {
                self.payload -= entry.bytes.len();
            } else {
                self.entries.shrink_to_fit();
                if self.bytes().saturating_add(extra) > self.budget {
                    self.clear();
                    return;
                }
                break;
            }
        }
        if self.anchored {
            let bytes = if self.current.len() == state.len() {
                encode(&self.current, state)
            } else {
                // Zero padding keeps length changes in the delta chain without
                // retaining a full predecessor for every changing-size frame.
                let length = self.current.len().max(state.len());
                let mut before = self.current.to_vec();
                before.resize(length, 0);
                let mut after = state.to_vec();
                after.resize(length, 0);
                encode(&before, &after)
            }
            .into_boxed_slice();
            self.payload += bytes.len();
            self.entries.push_back(Entry {
                bytes,
                previous_len: self.current.len(),
            });
        }
        self.current = state.into();
        self.anchored = true;
        while self.bytes() > self.budget {
            if let Some(entry) = self.entries.pop_front() {
                self.payload -= entry.bytes.len();
            } else {
                self.entries.shrink_to_fit();
                break;
            }
        }
    }
    /// Commit removal only after the core accepts the candidate. A failed
    /// transactional restore leaves both the machine and its history intact.
    pub fn restore<E>(&mut self, load: impl FnOnce(&[u8]) -> Result<(), E>) -> Result<bool, E> {
        let Some(entry) = self.entries.back() else {
            return Ok(false);
        };
        let mut candidate = self.current.to_vec();
        candidate.resize(self.current.len().max(entry.previous_len), 0);
        apply(&mut candidate, &entry.bytes);
        candidate.truncate(entry.previous_len);
        load(&candidate)?;
        self.current = candidate.into_boxed_slice();
        self.payload -= self.entries.pop_back().unwrap().bytes.len();
        Ok(true)
    }
}

fn varint(value: usize, out: &mut Vec<u8>) {
    let mut value = value;
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn read_varint(input: &[u8], at: &mut usize) -> usize {
    let mut value = 0usize;
    let mut shift = 0u32;
    while *at < input.len() {
        let byte = input[*at];
        *at += 1;
        value |= ((byte & 0x7f) as usize) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    value
}

/// `[skip][changed][changed bytes]...` over `a ^ b`, to the end of the buffers.
fn encode(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < a.len() {
        let start = i;
        while i < a.len() && a[i] == b[i] {
            i += 1;
        }
        varint(i - start, &mut out);
        let changed = i;
        while i < a.len() && a[i] != b[i] {
            i += 1;
        }
        varint(i - changed, &mut out);
        out.extend((changed..i).map(|n| a[n] ^ b[n]));
    }
    out
}

/// XOR a delta back over a state. Self-inverse, so the same code walks either
/// direction along the chain.
fn apply(state: &mut [u8], delta: &[u8]) {
    let mut at = 0;
    let mut i = 0;
    while at < delta.len() {
        i += read_varint(delta, &mut at);
        let changed = read_varint(delta, &mut at);
        for _ in 0..changed {
            if i >= state.len() || at >= delta.len() {
                return;
            }
            state[i] ^= delta[at];
            i += 1;
            at += 1;
        }
    }
}
