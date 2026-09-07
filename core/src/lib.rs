//! `nes-core` - the emulation core for androidemu.
//!
//! The core is a pure function of `(state, input) -> (state, framebuffer, samples)`.
//! It performs no I/O, spawns no threads, and allocates nothing during a frame.
//! That property is what makes rewind, deterministic replay and rollback netplay
//! cheap to add later - see PLAN.md.

pub mod apu;
pub mod bus;
pub mod cart;
pub mod controller;
pub mod cpu;
pub mod nes;
pub mod ppu;
pub mod rewind;

pub use bus::NesBus;
pub use cart::{Cartridge, Mirroring, Region};
pub use controller::Buttons;
pub use cpu::Cpu;
pub use nes::{Nes, Trace};

pub mod state;

mod audio_filter;
