//! Rewind: an undo chain of machine states.
//!
//! Holding a full snapshot per frame would cost roughly 95 KB a frame, so each
//! entry is instead the XOR of one state against the next, run-length encoded
//! over its zero runs. Between consecutive frames most of the machine is
//! untouched and the encoding collapses to almost nothing; the exception is the
//! framebuffer during a full-screen scroll, where nearly every byte moves.
//!
//! That is why the window is bounded in *bytes* rather than frames. A quiet
//! screen buys a long rewind and a scrolling one buys a shorter one, which is
//! the right trade to make automatically rather than by picking a frame count
//! that is wrong in both directions.
//!
//! Walking backwards costs one delta. The chain is anchored at the newest state
//! it has seen, and XORing the newest delta into it yields the state before it -
//! the same operation in both directions, since `a ^ (a ^ b) == b`.

use std::collections::VecDeque;

pub struct Rewind {
    /// The most recent state pushed, mutated in place as the chain unwinds.
    current: Vec<u8>,
    /// Encoded deltas, oldest first. Each turns its successor into the state
    /// that preceded it.
    deltas: VecDeque<Vec<u8>>,
    bytes: usize,
    budget: usize,
}

impl Rewind {
    pub fn new(budget: usize) -> Rewind {
        Rewind { current: Vec::new(), deltas: VecDeque::new(), bytes: 0, budget }
    }

    pub fn clear(&mut self) {
        self.current.clear();
        self.deltas.clear();
        self.bytes = 0;
    }

    /// How many steps back the chain can still go.
    pub fn depth(&self) -> usize {
        self.deltas.len()
    }

    /// Encoded bytes held, excluding the anchor state.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Record the state the machine is in now. A state of a different length
    /// belongs to a different game, and restarts the chain.
    pub fn push(&mut self, state: &[u8]) {
        if self.current.len() != state.len() {
            self.clear();
            self.current.extend_from_slice(state);
            return;
        }
        let delta = encode(&self.current, state);
        self.current.copy_from_slice(state);
        // An unchanged frame encodes to a couple of bytes. Keeping it costs
        // nothing and keeps one pop equal to one frame.
        self.bytes += delta.len();
        self.deltas.push_back(delta);
        while self.bytes > self.budget {
            match self.deltas.pop_front() {
                // Dropping the oldest only shortens how far back the chain
                // reaches; it stays anchored at the newest state.
                Some(dropped) => self.bytes -= dropped.len(),
                None => break,
            }
        }
    }

    /// Step one state back, returning it, or `None` at the end of the chain.
    pub fn pop(&mut self) -> Option<&[u8]> {
        let delta = self.deltas.pop_back()?;
        self.bytes -= delta.len();
        apply(&mut self.current, &delta);
        Some(&self.current)
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
