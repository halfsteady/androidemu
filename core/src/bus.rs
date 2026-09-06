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

pub struct NesBus {
    pub ram: [u8; 2048],
    pub ppu: Ppu,
    pub apu: Apu,
    pub cart: Cartridge,
    pub controllers: [Controller; 2],

    /// Value last driven onto the CPU bus, returned for reads of open address space.
    open_bus: u8,

    /// Cycles the CPU is stalled by an in-progress OAM DMA. Drained by [`crate::nes::Nes`]
    /// after each instruction; the copy itself has already happened by then.
    pub dma_stall: u32,
}

impl NesBus {
    pub fn new(cart: Cartridge) -> NesBus {
        NesBus {
            ram: [0; 2048],
            ppu: Ppu::new(),
            apu: Apu::new(),
            cart,
            controllers: Default::default(),
            open_bus: 0,
            dma_stall: 0,
        }
    }

    /// Advance the rest of the system by one CPU cycle. The PPU runs at three dots
    /// per CPU cycle on NTSC.
    fn tick(&mut self) {
        for _ in 0..3 {
            self.ppu.tick();
        }
        self.apu.tick();
        self.cart.mapper.tick();
    }

    /// $4014: copy 256 bytes from CPU page `page` into OAM.
    ///
    /// On hardware the CPU is halted and the copy is interleaved with the PPU over
    /// 513 or 514 cycles. Here the copy is performed at once and the stall is
    /// reported to the caller, which keeps the PPU catch-up correct to within the
    /// alignment cycle. Cycle-exact interleaving lands with the renderer in Phase 1.
    fn oam_dma(&mut self, page: u8) {
        let base = (page as u16) << 8;
        for i in 0..256u16 {
            let v = self.read_pure(base + i);
            let addr = self.ppu.oam_addr;
            self.ppu.oam[addr as usize] = v;
            self.ppu.oam_addr = addr.wrapping_add(1);
        }
        self.dma_stall = 513;
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
        self.tick();
        let v = match addr {
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize],
            0x2000..=0x3FFF => self.ppu.read_register(addr),
            0x4015 => self.apu.read_register(addr),
            0x4016 => (self.open_bus & 0xE0) | self.controllers[0].read(),
            0x4017 => (self.open_bus & 0xE0) | self.controllers[1].read(),
            // $4000-$4014 are write-only, and $4018-$401F are disabled.
            0x4000..=0x401F => self.open_bus,
            0x4020..=0xFFFF => self.cart.mapper.cpu_read(addr).unwrap_or(self.open_bus),
        };
        self.open_bus = v;
        v
    }

    fn write(&mut self, addr: u16, val: u8) {
        self.tick();
        self.open_bus = val;
        match addr {
            0x0000..=0x1FFF => self.ram[(addr & 0x07FF) as usize] = val,
            0x2000..=0x3FFF => self.ppu.write_register(addr, val),
            0x4014 => self.oam_dma(val),
            0x4016 => {
                self.controllers[0].write_strobe(val);
                self.controllers[1].write_strobe(val);
            }
            0x4000..=0x4013 | 0x4015 | 0x4017 => self.apu.write_register(addr, val),
            0x4018..=0x401F => {}
            0x4020..=0xFFFF => self.cart.mapper.cpu_write(addr, val),
        }
    }

    fn nmi_line(&self) -> bool {
        self.ppu.nmi_line
    }

    fn irq_line(&self) -> bool {
        self.apu.irq_line() || self.cart.mapper.irq()
    }
}
