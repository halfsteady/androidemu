//! The assembled machine.

use crate::cart::{CartError, Cartridge};
use crate::controller::Buttons;
use crate::cpu::{disasm, Bus, Cpu};
use crate::ppu::{HEIGHT, WIDTH};
use crate::NesBus;

/// One line of execution trace: the CPU state *before* an instruction ran.
///
/// Deliberately structured rather than formatted. The nestest harness compares
/// these fields against the reference log, which is far more robust than matching
/// log text and pinpoints the failure to a single register.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trace {
    pub pc: u16,
    pub opcode: u8,
    pub bytes: [u8; 3],
    pub len: u16,
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub p: u8,
    pub s: u8,
    pub cycles: u64,
    pub ppu_dot: u16,
    pub ppu_scanline: u16,
}

impl Trace {
    /// Render in the nestest.log layout, for eyeballing a diff.
    pub fn format(&self) -> String {
        let mut hex = String::new();
        for i in 0..3 {
            if (i as u16) < self.len {
                hex.push_str(&format!("{:02X} ", self.bytes[i]));
            } else {
                hex.push_str("   ");
            }
        }
        let text = disasm::disasm(self.pc, &self.bytes);
        format!(
            "{:04X}  {} {:<31} A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X} PPU:{:3},{:3} CYC:{}",
            self.pc,
            hex.trim_end(),
            text,
            self.a,
            self.x,
            self.y,
            self.p,
            self.s,
            self.ppu_scanline,
            self.ppu_dot,
            self.cycles
        )
    }
}

pub struct Nes {
    pub cpu: Cpu,
    pub bus: NesBus,
}

impl Nes {
    pub fn new(rom: &[u8]) -> Result<Nes, CartError> {
        let cart = Cartridge::load(rom)?;
        let mut nes = Nes {
            cpu: Cpu::new(),
            bus: NesBus::new(cart),
        };
        nes.reset();
        Ok(nes)
    }

    pub fn reset(&mut self) {
        self.cpu.reset(&mut self.bus);
    }

    pub fn set_buttons(&mut self, port: usize, buttons: Buttons) {
        self.bus.controllers[port].set_buttons(buttons);
    }

    /// Run one instruction. Returns the cycles it consumed, OAM DMA included.
    pub fn step(&mut self) -> u64 {
        let mut cycles = self.cpu.step(&mut self.bus);
        cycles += self.drain_dma();
        cycles
    }

    /// An OAM DMA halts the CPU. The copy has already been performed by the bus, so
    /// what is left is to burn the cycles and let the PPU catch up.
    fn drain_dma(&mut self) -> u64 {
        let stall = core::mem::take(&mut self.bus.dma_stall);
        if stall == 0 {
            return 0;
        }
        for _ in 0..stall {
            // Reading open bus advances the system by one cycle without side effects.
            self.bus.read(0x4020);
        }
        self.cpu.cycles += stall as u64;
        stall as u64
    }

    /// Step one instruction, capturing the CPU state as it was beforehand.
    pub fn step_traced(&mut self) -> Trace {
        let pc = self.cpu.pc;
        let mut bytes = [0u8; 3];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = self.bus.read_pure(pc.wrapping_add(i as u16));
        }
        let trace = Trace {
            pc,
            opcode: bytes[0],
            bytes,
            len: disasm::length(bytes[0]),
            a: self.cpu.a,
            x: self.cpu.x,
            y: self.cpu.y,
            p: self.cpu.p,
            s: self.cpu.s,
            cycles: self.cpu.cycles,
            ppu_dot: self.bus.ppu.dot,
            ppu_scanline: self.bus.ppu.scanline,
        };
        self.step();
        trace
    }

    /// Run until the PPU completes a frame. Returns the indexed framebuffer.
    pub fn step_frame(&mut self) -> &[u8] {
        let target = self.bus.ppu.frame + 1;
        // A frame is ~29,780 CPU cycles; the bound is a safety net against a jammed
        // CPU spinning here forever.
        let mut guard = 200_000;
        while self.bus.ppu.frame < target && guard > 0 {
            self.step();
            guard -= 1;
        }
        &self.bus.ppu.framebuffer
    }

    pub fn framebuffer(&self) -> &[u8] {
        &self.bus.ppu.framebuffer
    }

    pub fn frame_size() -> (usize, usize) {
        (WIDTH, HEIGHT)
    }
}
