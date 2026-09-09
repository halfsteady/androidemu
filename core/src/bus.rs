//! The CPU address space, and the wiring between CPU, PPU, APU, cartridge and
//! controllers.
//!
//! ```text
//!   $0000-$07FF  2 KB work RAM        (mirrored to $1FFF)
//!   $2000-$2007  PPU registers        (mirrored to $3FFF)
//!   $4000-$4017  APU and I/O
//!   $4018-$401F  disabled test registers
//!   $4020-$FFFF  cartridge
//! ```

use crate::apu::Apu;
use crate::cart::Cartridge;
use crate::controller::Controller;
use crate::cpu::Bus;
use crate::ppu::Ppu;

#[derive(Clone)]
pub struct NesBus {
    pub ram: [u8; 2048],
    pub ppu: Ppu,
    pub apu: Apu,
    pub cart: Cartridge,
    pub controllers: [Controller; 2],

    /// Value last driven onto the CPU bus, returned for reads of open address space.
    open_bus: u8,
    /// CPU-side latch, isolated from external DMA data during $4015 reads.
    internal_bus: u8,

    /// Cycles the CPU is stalled by an in-progress OAM DMA. Drained by [`crate::nes::Nes`]
    /// after each instruction, interleaving the actual copy with device clocks.
    pub dma_stall: u32,
    pub(crate) dma_page: u8,
    cycles: u64,
    pub(crate) extra_cycles: u32,
    // A property of the last access, consumed within the current instruction.
    // It is overwritten before use after a snapshot, so it is not serialized.
    last_read_halted: bool,
}

impl NesBus {
    pub fn new(cart: Cartridge) -> NesBus {
        let mut ppu = Ppu::new(); ppu.region = cart.header.region;
        let mut apu = Apu::new(); apu.region = cart.header.region;
        NesBus {
            ram: [0; 2048],
            ppu,
            apu,
            cart,
            controllers: Default::default(),
            open_bus: 0,
            internal_bus: 0,
            dma_stall: 0,
            dma_page: 0,
            cycles: 0,
            extra_cycles: 0,
            last_read_halted: false,
        }
    }

    /// Advance the rest of the system by one CPU cycle. The PPU runs at three dots
    /// per CPU cycle on NTSC.
    fn tick(&mut self) {
        self.begin_cycle();
        self.end_cycle();
    }

    fn end_cycle(&mut self) {
        if self.cycles & 1 == 0 {
            for controller in &mut self.controllers { controller.clock_strobe(); }
        }
        self.ppu.tick(self.cart.mapper.as_mut());
        if self.cart.header.region == crate::Region::Pal && self.cycles.is_multiple_of(5) { self.ppu.tick(self.cart.mapper.as_mut()); }
    }

    fn begin_cycle(&mut self) {
        self.cycles = self.cycles.wrapping_add(1);
        for _ in 0..2 {
            self.ppu.tick(self.cart.mapper.as_mut());
        }
        self.apu.tick();
        self.cart.mapper.tick();
    }

    /// Queue OAM DMA. The assembled machine interleaves reads and writes with
    /// PPU/APU clocks after the writing instruction completes.
    fn oam_dma(&mut self, page: u8) {
        self.dma_page = page;
        self.dma_stall = 513 + (self.cycles as u32 & 1);
    }

    fn dmc_dma(&mut self) {
        if let Some(addr) = self.apu.dmc_request() {
            // DMA halts on a CPU read, then spends a dummy cycle and an optional
            // alignment cycle before fetching. Initial-load scheduling, DMC/OAM
            // arbitration and repeated I/O reads remain incomplete.
            let stall = 3 + (self.cycles as u32 & 1);
            for _ in 0..stall - 1 {
                self.tick();
            }
            self.begin_cycle();
            let byte = self.cart.mapper.cpu_read(addr).unwrap_or(self.open_bus);
            self.open_bus = byte;
            self.apu.supply_dmc(byte);
            self.end_cycle();
            self.extra_cycles += stall;
        }
    }

    /// A read with no side effects on the PPU or APU, for DMA and for debuggers.
    pub fn read_pure(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize],
            0x2000..=0x3FFF => self.open_bus,
            0x4020..=0xFFFF => self.cart.mapper.cpu_read(addr).unwrap_or(self.open_bus),
            _ => self.open_bus,
        }
    }
}

impl Bus for NesBus {
    fn read(&mut self, addr: u16) -> u8 {
        let extra_before = self.extra_cycles;
        self.dmc_dma();
        self.last_read_halted = self.extra_cycles != extra_before;
        self.begin_cycle();
        let v = match addr {
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize],
            0x2000..=0x3FFF => self.ppu.read_register(addr, self.cart.mapper.as_mut()),
            0x4015 => self.apu.read_register(addr) | (self.internal_bus & 0x20),
            0x4016 => (self.open_bus & 0xE0) | self.controllers[0].read(),
            0x4017 => (self.open_bus & 0xE0) | self.controllers[1].read(),
            // $4000-$4014 are write-only, and $4018-$401F are disabled.
            0x4000..=0x401F => self.open_bus,
            0x4020..=0xFFFF => self.cart.mapper.cpu_read(addr).unwrap_or(self.open_bus),
        };
        // APU status is driven on the CPU's internal bus, without changing the
        // external data-bus latch. Its unconnected bit 5 remains open bus.
        if addr != 0x4015 { self.open_bus = v; }
        self.internal_bus = v;
        self.end_cycle();
        v
    }

    fn write(&mut self, addr: u16, val: u8) {
        self.last_read_halted = false;
        self.begin_cycle();
        self.open_bus = val;
        self.internal_bus = val;
        match addr {
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize] = val,
            0x2000..=0x3FFF => self
                .ppu
                .write_register(addr, val, self.cart.mapper.as_mut()),
            0x4014 => self.oam_dma(val),
            0x4016 => {
                self.controllers[0].set_strobe_line(val);
                self.controllers[1].set_strobe_line(val);
            }
            0x4000..=0x4013 | 0x4015 | 0x4017 => self.apu.write_register(addr, val),
            0x4018..=0x401F => {}
            0x4020..=0xFFFF => self.cart.mapper.cpu_write(addr, val),
        }
        self.end_cycle();
    }

    fn nmi_line(&self) -> bool {
        self.ppu.nmi_line
    }

    fn read_halted(&self) -> bool {
        self.last_read_halted
    }

    fn irq_line(&self) -> bool {
        self.apu.irq_line() || self.cart.mapper.irq()
    }
}

impl NesBus {
    pub(crate) fn save_state(&self, out: &mut Vec<u8>) {
        use crate::state::Codec;
        self.ram.encode(out);
        self.ppu.encode(out);
        self.apu.encode(out);
        self.controllers.encode(out);
        self.open_bus.encode(out);
        self.dma_stall.encode(out);
        self.dma_page.encode(out);
        self.cycles.encode(out);
        self.extra_cycles.encode(out);
    }
    pub(crate) fn load_state(&mut self, input: &mut &[u8]) -> crate::state::Result<()> {
        use crate::state::{Codec, StateError};
        self.ram = Codec::decode(input)?;
        self.ppu = Codec::decode(input)?;
        self.ppu.initialize_legacy_accuracy_state();
        self.apu = Codec::decode(input)?;
        self.controllers = Codec::decode(input)?;
        self.open_bus = Codec::decode(input)?;
        self.internal_bus = self.open_bus;
        self.dma_stall = Codec::decode(input)?;
        self.dma_page = Codec::decode(input)?;
        self.cycles = Codec::decode(input)?;
        self.extra_cycles = Codec::decode(input)?;
        self.last_read_halted = false;
        // Timing configuration is immutable cartridge metadata, not serialized
        // device state. Keeping it out of the codec preserves preview-v1 saves.
        self.ppu.region = self.cart.header.region;
        self.apu.region = self.cart.header.region;
        if !self.ppu.valid_state()
            || !self.apu.valid_state()
            || self.dma_stall > 514
            || self.extra_cycles > 4096
        {
            return Err(StateError("Invalid machine state"));
        }
        Ok(())
    }

    pub(crate) fn save_accuracy_state(&self, out: &mut Vec<u8>) {
        use crate::state::Codec;
        self.internal_bus.encode(out);
        self.ppu.save_accuracy_state(out);
        self.apu.save_accuracy_state(out);
    }

    pub(crate) fn load_accuracy_state(&mut self, input: &mut &[u8]) -> crate::state::Result<()> {
        use crate::state::Codec;
        self.internal_bus = Codec::decode(input)?;
        self.ppu.load_accuracy_state(input)?;
        self.apu.load_accuracy_state(input)?;
        Ok(())
    }
}
