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

#[derive(Clone, Copy, PartialEq, Eq)]
enum DmcDmaStage {
    Idle,
    Halt,
    Dummy,
    Ready,
}

impl DmcDmaStage {
    fn advance(self, requested: bool) -> Self {
        match self {
            // Cancellation during halt releases RDY immediately. Once setup
            // has advanced, the remaining transfer clocks still stall the CPU.
            Self::Halt if !requested => Self::Idle,
            Self::Halt => Self::Dummy,
            Self::Dummy => Self::Ready,
            other => other,
        }
    }
}

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

    /// Nonzero while OAM DMA is queued for the next CPU read. The stored cycle
    /// estimate is retained for compatibility with the released snapshot layout;
    /// actual timing depends on the halt phase and any overlapping DMC DMA.
    pub dma_stall: u32,
    pub(crate) dma_page: u8,
    cycles: u64,
    pub(crate) extra_cycles: u32,
    // A property of the last access, consumed within the current instruction.
    // It is overwritten before use after a snapshot, so it is not serialized.
    last_read_halted: bool,
    controller_port: u8,
    controller_bit: u8,
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
            controller_port: 0,
            controller_bit: 0,
        }
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

    /// Queue OAM DMA. Writes (including the second write of an RMW instruction)
    /// continue until a CPU read can be halted by RDY.
    fn oam_dma(&mut self, page: u8) {
        self.dma_page = page;
        self.dma_stall = 513 + (self.cycles as u32 & 1);
    }

    /// Both DMA units share the same get/put clock. RDY halts only reads;
    /// DMC setup overlaps OAM transfers, and its get takes priority over OAM.
    fn run_dma(&mut self, cpu_addr: u16) {
        let mut oam = self.dma_stall != 0;
        let mut dmc_stage = if self.apu.dmc_request().is_some() { DmcDmaStage::Halt } else { DmcDmaStage::Idle };
        if !oam && dmc_stage == DmcDmaStage::Idle { return; }
        self.dma_stall = 0;
        let mut offset = 0u16;
        let mut oam_byte = None;
        // Successful RDY halt: both engines may share this cycle.
        self.begin_cycle();
        self.read_data(cpu_addr, cpu_addr, true);
        self.end_cycle();
        self.extra_cycles += 1;
        dmc_stage = dmc_stage.advance(self.apu.dmc_request().is_some());
        while oam || dmc_stage != DmcDmaStage::Idle || self.apu.dmc_request().is_some() {
            if dmc_stage == DmcDmaStage::Idle && self.apu.dmc_request().is_some() {
                dmc_stage = DmcDmaStage::Halt;
            }
            let get = (self.cycles + 1) & 1 == 0;
            let dmc_get = get && dmc_stage == DmcDmaStage::Ready;
            self.begin_cycle();
            if dmc_get {
                if let Some(address) = self.apu.dmc_request() {
                    let byte = self.read_data(address, cpu_addr, false);
                    self.apu.supply_dmc(byte);
                } else {
                    self.read_data(cpu_addr, cpu_addr, true);
                }
                dmc_stage = DmcDmaStage::Idle;
            } else if get && oam {
                oam_byte = Some(self.read_data(((self.dma_page as u16) << 8) | offset, cpu_addr, false));
            } else if !get && oam_byte.is_some() {
                let byte = oam_byte.take().unwrap();
                self.controller_port = 0;
                self.open_bus = byte;
                // OAM puts drive the CPU's write bus; DMC gets do not.
                self.internal_bus = byte;
                self.ppu.write_register(0x2004, byte, self.cart.mapper.as_mut());
                offset += 1;
                oam = offset != 256;
            } else {
                self.read_data(cpu_addr, cpu_addr, true);
            }
            self.end_cycle();
            self.extra_cycles += 1;
            dmc_stage = dmc_stage.advance(self.apu.dmc_request().is_some());
        }
    }

    fn read_data(&mut self, addr: u16, cpu_addr: u16, latch_cpu: bool) -> u8 {
        let external = match addr {
            0x0000..=0x1fff => Some(self.ram[(addr & 0x07ff) as usize]),
            0x2000..=0x3fff => Some(self.ppu.read_register(addr, self.cart.mapper.as_mut())),
            0x4020..=0xffff => self.cart.mapper.cpu_read(addr),
            _ => None,
        };
        // APU register selection combines the held CPU address's upper bits
        // with the DMA address's low five bits, even when they name ROM/RAM.
        let io = if cpu_addr & 0xffe0 == 0x4000 { 0x4000 | (addr & 0x1f) } else { 0 };
        let port = match io { 0x4016 => 1, 0x4017 => 2, _ => 0 };
        let mut value = external.unwrap_or(self.open_bus);
        if port != 0 {
            if self.controller_port != port {
                self.controller_bit = self.controllers[(port - 1) as usize].read();
            }
            value = (value & 0xe0) | self.controller_bit;
        }
        self.controller_port = port;
        self.open_bus = value;
        if io == 0x4015 {
            value = self.apu.read_register(0x4015) | (self.internal_bus & 0x20);
        }
        if latch_cpu { self.internal_bus = value; }
        // The CPU sees the controller input directly, but a DMA reader samples
        // the contested external pins. A cartridge/RAM zero wins on those pins.
        if !latch_cpu && port != 0 { value & external.unwrap_or(0xff) } else { value }
    }

    /// A read with no side effects on the PPU or APU, for debuggers.
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
        self.run_dma(addr);
        self.last_read_halted = self.extra_cycles != extra_before;
        self.begin_cycle();
        let v = self.read_data(addr, addr, true);
        self.end_cycle();
        v
    }

    fn write(&mut self, addr: u16, val: u8) {
        self.last_read_halted = false;
        self.controller_port = 0;
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
        self.controller_port = 0;
        self.controller_bit = 0;
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

    pub(crate) fn save_pipeline_state(&self, out: &mut Vec<u8>) {
        use crate::state::Codec;
        self.apu.save_pipeline_state(out);
        self.controller_port.encode(out);
        self.controller_bit.encode(out);
    }

    pub(crate) fn load_pipeline_state(&mut self, input: &mut &[u8]) -> crate::state::Result<()> {
        use crate::state::{Codec, StateError};
        self.apu.load_pipeline_state(input)?;
        self.controller_port = Codec::decode(input)?;
        self.controller_bit = Codec::decode(input)?;
        if self.controller_port > 2 || self.controller_bit > 1 { return Err(StateError("Invalid controller bus state")); }
        Ok(())
    }
}
