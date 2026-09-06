//! Audio Processing Unit (2A03).
//!
//! Phase 0 scope: the register interface and the frame counter, which is an IRQ
//! source the CPU has to see correctly even before any sound is generated. The five
//! channels and the band-limited mixer land in Phase 1 (PLAN.md §1).

pub struct Apu {
    regs: [u8; 0x18],
    /// $4017 bit 7: 0 = four-step sequence, 1 = five-step.
    five_step: bool,
    /// $4017 bit 6: inhibits the frame IRQ.
    irq_inhibit: bool,
    frame_irq: bool,
    cycle: u32,
}

impl Default for Apu {
    fn default() -> Self {
        Apu::new()
    }
}

impl Apu {
    pub fn new() -> Apu {
        Apu {
            regs: [0; 0x18],
            five_step: false,
            irq_inhibit: false,
            frame_irq: false,
            cycle: 0,
        }
    }

    pub fn irq_line(&self) -> bool {
        self.frame_irq
    }

    /// One APU tick per CPU cycle. The frame sequencer actually runs on the APU's
    /// half-rate clock; the step counts below are in CPU cycles to match.
    pub fn tick(&mut self) {
        self.cycle += 1;
        let period = if self.five_step { 37282 } else { 29830 };
        if self.cycle >= period {
            self.cycle = 0;
            if !self.five_step && !self.irq_inhibit {
                self.frame_irq = true;
            }
        }
    }

    pub fn read_register(&mut self, addr: u16) -> u8 {
        match addr {
            0x4015 => {
                // Reading the status register acknowledges the frame IRQ.
                let v = if self.frame_irq { 0x40 } else { 0 };
                self.frame_irq = false;
                v
            }
            _ => 0,
        }
    }

    pub fn write_register(&mut self, addr: u16, val: u8) {
        match addr {
            0x4000..=0x4013 => self.regs[(addr - 0x4000) as usize] = val,
            0x4015 => self.regs[0x15] = val,
            0x4017 => {
                self.five_step = val & 0x80 != 0;
                self.irq_inhibit = val & 0x40 != 0;
                if self.irq_inhibit {
                    self.frame_irq = false;
                }
                self.cycle = 0;
            }
            _ => {}
        }
    }
}
