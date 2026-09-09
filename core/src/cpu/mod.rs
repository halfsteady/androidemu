//! Cycle-accurate MOS 6502 (Ricoh 2A03) core.
//!
//! # Timing model
//!
//! Every cycle of a real 6502 is a bus cycle - the chip has no idle cycles, only
//! cycles whose read result is discarded. This core mirrors that: [`Bus::read`] and
//! [`Bus::write`] each advance the system by exactly one CPU cycle, and *every*
//! cycle of every instruction goes through one of them, dummy accesses included.
//!
//! The consequence is that cycle counts are **emergent, not tabulated**. There is no
//! table of instruction lengths to get wrong, and the PPU and APU see accesses land
//! on the same cycles they would on hardware - which is what mid-scanline register
//! writes and MMC3 IRQ timing depend on.

mod table;

pub use table::{Access, Mode, Op};


pub mod disasm;

/// The CPU's view of the system. Both accessors must advance the rest of the
/// machine by one CPU cycle before returning.
pub trait Bus {
    fn read(&mut self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, val: u8);

    /// Whether DMA held RDY low during the most recent read. Unstable stores
    /// observe this on their indexing dummy read, immediately before writing.
    fn read_halted(&self) -> bool {
        false
    }

    /// Level of the /NMI line, sampled by the CPU each cycle. The edge detection
    /// lives in the CPU, so this reports the raw level.
    fn nmi_line(&self) -> bool {
        false
    }
    /// Level of the /IRQ line (active high here). Level-triggered, so it stays
    /// asserted until the device is acknowledged.
    fn irq_line(&self) -> bool {
        false
    }
}

pub mod flags {
    pub const C: u8 = 1 << 0;
    pub const Z: u8 = 1 << 1;
    pub const I: u8 = 1 << 2;
    pub const D: u8 = 1 << 3;
    pub const B: u8 = 1 << 4;
    pub const U: u8 = 1 << 5; // physically unconnected; always reads as 1
    pub const V: u8 = 1 << 6;
    pub const N: u8 = 1 << 7;
}

const STACK_BASE: u16 = 0x0100;
const NMI_VECTOR: u16 = 0xFFFA;
const RESET_VECTOR: u16 = 0xFFFC;
const IRQ_VECTOR: u16 = 0xFFFE;

#[derive(Clone)]
pub struct Cpu {
    pub pc: u16,
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub s: u8,
    pub p: u8,

    /// Total CPU cycles since power-on. Wraps after ~3000 years at 1.79 MHz.
    pub cycles: u64,

    /// Edge detector for /NMI: the interrupt fires on a high-to-low transition,
    /// not on the level, so the previous sample has to be kept.
    nmi_prev: bool,
    nmi_pending: bool,
    irq_pending: bool,
    nmi_ready: bool,
    irq_ready: bool,

    /// True once a KIL/JAM opcode has locked the processor.
    pub jammed: bool,
}

impl Default for Cpu {
    fn default() -> Self {
        Cpu::new()
    }
}

impl Cpu {
    pub fn new() -> Cpu {
        Cpu {
            pc: 0,
            a: 0,
            x: 0,
            y: 0,
            s: 0xFD,
            p: flags::I | flags::U,
            cycles: 0,
            nmi_prev: false,
            nmi_pending: false,
            irq_pending: false,
            nmi_ready: false,
            irq_ready: false,
            jammed: false,
        }
    }

    /// Power-on / reset sequence. Consumes 7 cycles like the hardware does.
    pub fn reset<B: Bus>(&mut self, bus: &mut B) {
        self.jammed = false;
        // Reset performs the same sequence as an interrupt, but the three "pushes"
        // are reads: the stack pointer decrements without anything being stored.
        self.read(bus, self.pc);
        self.read(bus, self.pc);
        self.read(bus, STACK_BASE | self.s as u16);
        self.s = self.s.wrapping_sub(1);
        self.read(bus, STACK_BASE | self.s as u16);
        self.s = self.s.wrapping_sub(1);
        self.read(bus, STACK_BASE | self.s as u16);
        self.s = self.s.wrapping_sub(1);
        self.p |= flags::I;
        let lo = self.read(bus, RESET_VECTOR) as u16;
        let hi = self.read(bus, RESET_VECTOR + 1) as u16;
        self.pc = lo | (hi << 8);
    }

    /// Force the program counter, for test ROMs that start in automation mode.
    pub fn set_pc(&mut self, pc: u16) {
        self.pc = pc;
    }

    // ---- bus access; every one of these is one CPU cycle ----

    fn read<B: Bus>(&mut self, bus: &mut B, addr: u16) -> u8 {
        let v = bus.read(addr);
        self.cycles += 1;
        self.sample_interrupts(bus);
        v
    }

    fn write<B: Bus>(&mut self, bus: &mut B, addr: u16, val: u8) {
        bus.write(addr, val);
        self.cycles += 1;
        self.sample_interrupts(bus);
    }

    /// Sampled at the end of every cycle, exactly as the hardware latches the lines.
    fn sample_interrupts<B: Bus>(&mut self, bus: &B) {
        // Interrupt polling uses the signal from the preceding cycle, so an
        // edge on the final cycle waits through the next instruction.
        self.nmi_ready = self.nmi_pending;
        self.irq_ready = self.irq_pending && !self.flag(flags::I);
        let nmi = bus.nmi_line();
        if nmi && !self.nmi_prev {
            self.nmi_pending = true;
        }
        self.nmi_prev = nmi;
        self.irq_pending = bus.irq_line();
    }

    fn fetch<B: Bus>(&mut self, bus: &mut B) -> u8 {
        let v = self.read(bus, self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }

    fn fetch16<B: Bus>(&mut self, bus: &mut B) -> u16 {
        let lo = self.fetch(bus) as u16;
        let hi = self.fetch(bus) as u16;
        lo | (hi << 8)
    }

    fn push<B: Bus>(&mut self, bus: &mut B, val: u8) {
        let addr = STACK_BASE | self.s as u16;
        self.write(bus, addr, val);
        self.s = self.s.wrapping_sub(1);
    }

    fn pull<B: Bus>(&mut self, bus: &mut B) -> u8 {
        self.s = self.s.wrapping_add(1);
        let addr = STACK_BASE | self.s as u16;
        self.read(bus, addr)
    }

    // ---- flags ----

    #[inline]
    fn set_flag(&mut self, mask: u8, on: bool) {
        if on {
            self.p |= mask;
        } else {
            self.p &= !mask;
        }
    }

    #[inline]
    fn flag(&self, mask: u8) -> bool {
        self.p & mask != 0
    }

    #[inline]
    fn set_zn(&mut self, v: u8) {
        self.set_flag(flags::Z, v == 0);
        self.set_flag(flags::N, v & 0x80 != 0);
    }

    // ---- interrupts ----

    /// The shared IRQ/NMI/BRK entry sequence: 7 cycles.
    fn interrupt<B: Bus>(&mut self, bus: &mut B, vector: u16, brk: bool) {
        // Two dummy reads. For BRK the PC has already been advanced past the
        // signature byte by the caller.
        if !brk {
            self.read(bus, self.pc);
            self.read(bus, self.pc);
        }
        self.push(bus, (self.pc >> 8) as u8);
        self.push(bus, self.pc as u8);

        // B is a *pushed* value, not a real register bit: set for BRK/PHP, clear
        // when the push is caused by hardware.
        let pushed = if brk {
            self.p | flags::B | flags::U
        } else {
            (self.p | flags::U) & !flags::B
        };
        self.push(bus, pushed);
        self.p |= flags::I;

        // An NMI edge arriving during the stack pushes hijacks IRQ/BRK's vector,
        // but does not change the return address or the already-pushed B flag.
        let vector = if vector == IRQ_VECTOR && self.nmi_ready {
            self.nmi_pending = false;
            self.nmi_ready = false;
            NMI_VECTOR
        } else { vector };
        let lo = self.read(bus, vector) as u16;
        let hi = self.read(bus, vector + 1) as u16;
        self.pc = lo | (hi << 8);
        // Vector fetches sample edges but are not instruction interrupt polls.
        // A later NMI runs after the handler's first instruction, not before it.
        self.nmi_ready = false;
        self.irq_ready = false;
    }

    /// Execute one instruction, or service a pending interrupt. Returns the number
    /// of cycles consumed.
    pub fn step<B: Bus>(&mut self, bus: &mut B) -> u64 {
        let start = self.cycles;

        // Re-sample the interrupt lines before deciding. Within a running system a
        // device always changes them during a bus cycle, where `read`/`write`
        // already catch it - but a line driven from outside the CPU (a test, a
        // debugger, a netplay resync) would otherwise go unseen for an instruction.
        // The NMI edge detector makes this idempotent.
        let nmi = bus.nmi_line();
        if nmi && !self.nmi_prev { self.nmi_pending = true; self.nmi_ready = true; }
        self.nmi_prev = nmi;
        if bus.irq_line() != self.irq_pending {
            self.irq_pending = bus.irq_line();
            self.irq_ready = self.irq_pending && !self.flag(flags::I);
        }

        if self.jammed {
            // A jammed CPU still burns cycles on the bus.
            self.read(bus, self.pc);
            return self.cycles - start;
        }

        if self.nmi_ready {
            self.nmi_pending = false;
            self.nmi_ready = false;
            self.interrupt(bus, NMI_VECTOR, false);
            return self.cycles - start;
        }
        if self.irq_ready {
            self.interrupt(bus, IRQ_VECTOR, false);
            return self.cycles - start;
        }

        let opcode = self.fetch(bus);
        self.execute(bus, opcode);
        self.cycles - start
    }

    /// Resolve the addressing mode to an effective address, performing exactly the
    /// dummy accesses the hardware performs.
    fn resolve<B: Bus>(&mut self, bus: &mut B, mode: Mode, access: Access) -> u16 {
        // A read that does not cross a page boundary skips the extra cycle; a write
        // or read-modify-write always pays it, because the CPU cannot know the
        // carry result in time to suppress the access.
        let always_penalty = matches!(access, Access::Write | Access::Rmw);

        match mode {
            Mode::Imp | Mode::Acc => {
                // The discarded operand fetch. This is the second cycle of every
                // implied instruction.
                self.read(bus, self.pc);
                0
            }
            Mode::Imm => {
                let a = self.pc;
                self.pc = self.pc.wrapping_add(1);
                a
            }
            Mode::Zp => self.fetch(bus) as u16,
            Mode::ZpX => {
                let base = self.fetch(bus);
                self.read(bus, base as u16); // dummy read at the un-indexed address
                base.wrapping_add(self.x) as u16
            }
            Mode::ZpY => {
                let base = self.fetch(bus);
                self.read(bus, base as u16);
                base.wrapping_add(self.y) as u16
            }
            Mode::Abs => self.fetch16(bus),
            Mode::AbsX => self.indexed_abs(bus, self.x, always_penalty),
            Mode::AbsY => self.indexed_abs(bus, self.y, always_penalty),
            Mode::Ind => {
                let ptr = self.fetch16(bus);
                let lo = self.read(bus, ptr) as u16;
                // The famous page-wrap bug: JMP ($xxFF) reads its high byte from
                // $xx00, not from the next page.
                let hi_addr = (ptr & 0xFF00) | (ptr.wrapping_add(1) & 0x00FF);
                let hi = self.read(bus, hi_addr) as u16;
                lo | (hi << 8)
            }
            Mode::IndX => {
                let base = self.fetch(bus);
                self.read(bus, base as u16);
                let ptr = base.wrapping_add(self.x);
                let lo = self.read(bus, ptr as u16) as u16;
                let hi = self.read(bus, ptr.wrapping_add(1) as u16) as u16;
                lo | (hi << 8)
            }
            Mode::IndY => {
                let ptr = self.fetch(bus);
                let lo = self.read(bus, ptr as u16) as u16;
                let hi = self.read(bus, ptr.wrapping_add(1) as u16) as u16;
                let base = lo | (hi << 8);
                let addr = base.wrapping_add(self.y as u16);
                if always_penalty || page_crossed(base, addr) {
                    self.read(bus, (base & 0xFF00) | (addr & 0x00FF));
                }
                addr
            }
            Mode::Rel => {
                let a = self.pc;
                self.pc = self.pc.wrapping_add(1);
                a
            }
        }
    }

    fn indexed_abs<B: Bus>(&mut self, bus: &mut B, index: u8, always: bool) -> u16 {
        let base = self.fetch16(bus);
        let addr = base.wrapping_add(index as u16);
        if always || page_crossed(base, addr) {
            // The dummy read uses the *un-carried* high byte - the address the CPU
            // put on the bus before the carry propagated.
            self.read(bus, (base & 0xFF00) | (addr & 0x00FF));
        }
        addr
    }

    fn branch<B: Bus>(&mut self, bus: &mut B, addr: u16, take: bool) {
        let offset = self.read(bus, addr) as i8;
        if !take {
            return;
        }
        let early_nmi = self.nmi_ready;
        let early_irq = self.irq_ready;
        // Taken branch: one cycle to add the offset...
        self.read(bus, self.pc);
        let target = (self.pc as i32 + offset as i32) as u16;
        if page_crossed(self.pc, target) {
            // ...and one more to fix up the high byte if it carried.
            self.read(bus, (self.pc & 0xFF00) | (target & 0x00FF));
            // Page-crossing branches poll again, but an interrupt recognized
            // by the first poll remains latched even if the line was cleared.
            self.nmi_ready |= early_nmi;
            self.irq_ready |= early_irq;
        } else {
            // A taken branch's third cycle does not poll for interrupts.
            self.nmi_ready = early_nmi;
            self.irq_ready = early_irq;
        }
        self.pc = target;
    }
}

#[inline]
fn page_crossed(a: u16, b: u16) -> bool {
    (a & 0xFF00) != (b & 0xFF00)
}

mod exec;

crate::state::state_fields!(Cpu, pc, a, x, y, s, p, cycles, nmi_prev, nmi_pending, irq_pending, nmi_ready, irq_ready, jammed);
