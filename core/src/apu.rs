//! NTSC 2A03 audio. Five channels, nonlinear mixer and fixed-rate output.
//! The per-cycle nonlinear mixer is band-limited by a fixed FIR before 48 kHz
//! decimation, then filtered for DC removal and the analog output response.
use crate::cart::Region;
const PAL_NOISE: [u16; 16] = [4, 8, 14, 30, 60, 88, 118, 148, 188, 236, 354, 472, 708, 944, 1890, 3778];
const PAL_DMC: [u16; 16] = [398,354,316,298,276,236,210,198,176,148,132,118,98,78,66,50];
pub const SAMPLE_RATE: u32 = 48_000;
const LENGTH: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];
const NOISE: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];
const DMC_RATE: [u16; 16] = [
    428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
];
const DUTY: [u8; 4] = [0b00000010, 0b00000110, 0b00011110, 0b11111001];
#[derive(Clone, Default)]
struct Envelope {
    start: bool,
    divider: u8,
    decay: u8,
}
impl Envelope {
    fn clock(&mut self, reg: u8) {
        if self.start {
            self.start = false;
            self.decay = 15;
            self.divider = reg & 15;
        } else if self.divider > 0 {
            self.divider -= 1;
        } else {
            self.divider = reg & 15;
            if self.decay > 0 {
                self.decay -= 1;
            } else if reg & 0x20 != 0 {
                self.decay = 15;
            }
        }
    }
    fn volume(&self, reg: u8) -> u8 {
        if reg & 0x10 != 0 {
            reg & 15
        } else {
            self.decay
        }
    }
}
#[derive(Clone, Default)]
struct Pulse {
    timer: u16,
    period: u16,
    sequence: u8,
    length: u8,
    envelope: Envelope,
    sweep_divider: u8,
    sweep_reload: bool,
}
impl Pulse {
    fn target(&self, reg: u8, first: bool) -> i32 {
        let change = (self.period >> (reg & 7)) as i32;
        if reg & 8 != 0 {
            self.period as i32 - change - i32::from(first)
        } else {
            self.period as i32 + change
        }
    }
    fn half(&mut self, volume: u8, sweep: u8, first: bool) {
        if volume & 0x20 == 0 && self.length > 0 {
            self.length -= 1;
        }
        let target = self.target(sweep, first);
        if self.sweep_divider == 0
            && sweep & 0x80 != 0
            && sweep & 7 != 0
            && self.period >= 8
            && (0..=0x7ff).contains(&target)
        {
            self.period = target as u16;
        }
        if self.sweep_divider == 0 || self.sweep_reload {
            self.sweep_divider = (sweep >> 4) & 7;
            self.sweep_reload = false;
        } else {
            self.sweep_divider -= 1;
        }
    }
    fn output(&self, volume: u8, sweep: u8, first: bool) -> u8 {
        if self.length == 0
            || self.period < 8
            || self.target(sweep, first) > 0x7ff
            || DUTY[(volume >> 6) as usize] & (1 << self.sequence) == 0
        {
            0
        } else {
            self.envelope.volume(volume)
        }
    }
}
#[derive(Clone)]
pub struct Apu {
    pub(crate) region: Region,
    regs: [u8; 0x18],
    five_step: bool,
    irq_inhibit: bool,
    frame_irq: bool,
    cycle: u32,
    total: u64,
    reset_delay: u8,
    pulse: [Pulse; 2],
    triangle_timer: u16,
    triangle_period: u16,
    triangle_sequence: u8,
    triangle_length: u8,
    linear: u8,
    linear_reload: bool,
    noise_timer: u16,
    noise_shift: u16,
    noise_length: u8,
    noise_envelope: Envelope,
    dmc_timer: u16,
    dmc_address: u16,
    dmc_remaining: u16,
    dmc_buffer: u8,
    dmc_full: bool,
    dmc_shift: u8,
    dmc_bits: u8,
    dmc_silent: bool,
    dmc_output: u8,
    dmc_irq: bool,
    sample_phase: u32,
    history: [f32; 1024],
    history_pos: usize,
    previous: f32,
    high_pass: f32,
    low_pass: f32,
    samples: [f32; 2048],
    sample_count: usize,
}
impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}
impl Apu {
    pub fn new() -> Self {
        Self {
            region: Region::Ntsc,
            regs: [0; 0x18],
            five_step: false,
            irq_inhibit: false,
            frame_irq: false,
            cycle: 0,
            total: 0,
            reset_delay: 0,
            pulse: Default::default(),
            triangle_timer: 0,
            triangle_period: 0,
            triangle_sequence: 0,
            triangle_length: 0,
            linear: 0,
            linear_reload: false,
            noise_timer: 0,
            noise_shift: 1,
            noise_length: 0,
            noise_envelope: Default::default(),
            dmc_timer: 0,
            dmc_address: 0xc000,
            dmc_remaining: 0,
            dmc_buffer: 0,
            dmc_full: false,
            dmc_shift: 0,
            dmc_bits: 8,
            dmc_silent: true,
            dmc_output: 0,
            dmc_irq: false,
            sample_phase: 0,
            history: [0.0; 1024],
            history_pos: 0,
            previous: 0.0,
            high_pass: 0.0,
            low_pass: 0.0,
            samples: [0.0; 2048],
            sample_count: 0,
        }
    }
    pub fn irq_line(&self) -> bool {
        self.frame_irq || self.dmc_irq
    }
    pub fn samples(&self) -> &[f32] {
        &self.samples[..self.sample_count]
    }
    pub fn clear_samples(&mut self) {
        self.sample_count = 0;
    }
    fn quarter(&mut self) {
        for i in 0..2 {
            self.pulse[i].envelope.clock(self.regs[i * 4]);
        }
        self.noise_envelope.clock(self.regs[0x0c]);
        if self.linear_reload {
            self.linear = self.regs[8] & 0x7f;
        } else if self.linear > 0 {
            self.linear -= 1;
        }
        if self.regs[8] & 0x80 == 0 {
            self.linear_reload = false;
        }
    }
    fn half(&mut self) {
        for i in 0..2 {
            self.pulse[i].half(self.regs[i * 4], self.regs[i * 4 + 1], i == 0);
        }
        if self.regs[8] & 0x80 == 0 && self.triangle_length > 0 {
            self.triangle_length -= 1;
        }
        if self.regs[0x0c] & 0x20 == 0 && self.noise_length > 0 {
            self.noise_length -= 1;
        }
    }
    pub fn tick(&mut self) {
        self.total = self.total.wrapping_add(1);
        self.cycle += 1;
        if self.reset_delay > 0 {
            self.reset_delay -= 1;
            if self.reset_delay == 0 {
                self.cycle = 0;
                if self.five_step {
                    self.quarter();
                    self.half();
                }
            }
        }
        let clocks = if self.region == Region::Pal { [8313, 16627, 24939, 33253, 41565] } else { [7457, 14913, 22371, 29829, 37281] };
        if self.cycle == clocks[0] || self.cycle == clocks[2] {
            self.quarter();
        }
        if self.cycle == clocks[1]
            || (!self.five_step && self.cycle == clocks[3])
            || (self.five_step && self.cycle == clocks[4])
        {
            self.quarter();
            self.half();
        }
        if !self.five_step && (clocks[3] - 1..=clocks[3] + 1).contains(&self.cycle) && !self.irq_inhibit {
            self.frame_irq = true;
        }
        if self.cycle >= if self.five_step { clocks[4] + 1 } else { clocks[3] + 1 } {
            self.cycle = 0;
        }
        if self.total & 1 == 0 {
            for p in &mut self.pulse {
                if p.timer == 0 {
                    p.timer = p.period;
                    p.sequence = (p.sequence + 1) & 7;
                } else {
                    p.timer -= 1;
                }
            }
        }
        if self.triangle_timer == 0 {
            self.triangle_timer = self.triangle_period;
            if self.triangle_length > 0 && self.linear > 0 && self.triangle_period > 1 {
                self.triangle_sequence = (self.triangle_sequence + 1) & 31;
            }
        } else {
            self.triangle_timer -= 1;
        }
        if self.noise_timer == 0 {
            self.noise_timer = (if self.region == Region::Pal { PAL_NOISE } else { NOISE })[(self.regs[0x0e] & 15) as usize] - 1;
            let tap = if self.regs[0x0e] & 0x80 != 0 { 6 } else { 1 };
            let feedback = (self.noise_shift ^ (self.noise_shift >> tap)) & 1;
            self.noise_shift = (self.noise_shift >> 1) | (feedback << 14);
        } else {
            self.noise_timer -= 1;
        }
        if self.dmc_timer == 0 {
            self.dmc_timer = (if self.region == Region::Pal { PAL_DMC } else { DMC_RATE })[(self.regs[0x10] & 15) as usize] - 1;
            if !self.dmc_silent {
                if self.dmc_shift & 1 != 0 {
                    if self.dmc_output <= 125 {
                        self.dmc_output += 2;
                    }
                } else if self.dmc_output >= 2 {
                    self.dmc_output -= 2;
                }
            }
            self.dmc_shift >>= 1;
            self.dmc_bits -= 1;
            if self.dmc_bits == 0 {
                self.dmc_bits = 8;
                self.dmc_silent = !self.dmc_full;
                if self.dmc_full {
                    self.dmc_shift = self.dmc_buffer;
                    self.dmc_full = false;
                }
            }
        } else {
            self.dmc_timer -= 1;
        }
        let pulse = (self.pulse[0].output(self.regs[0], self.regs[1], true)
            + self.pulse[1].output(self.regs[4], self.regs[5], false)) as f64;
        // Triangle holds its DAC value when its counters stop.
        let tri = if self.triangle_sequence < 16 {
            15 - self.triangle_sequence
        } else {
            self.triangle_sequence - 16
        } as f64;
        let noise = if self.noise_length > 0 && self.noise_shift & 1 == 0 {
            self.noise_envelope.volume(self.regs[0x0c])
        } else {
            0
        } as f64;
        let tnd = tri / 8227.0 + noise / 12241.0 + self.dmc_output as f64 / 22638.0;
        let mixed = if pulse == 0.0 {
            0.0
        } else {
            95.88 / (8128.0 / pulse + 100.0)
        } + if tnd == 0.0 {
            0.0
        } else {
            159.79 / (1.0 / tnd + 100.0)
        };
        self.history[self.history_pos] = mixed as f32;
        self.history_pos = (self.history_pos + 1) & 1023;
        self.sample_phase += SAMPLE_RATE;
        if self.sample_phase >= self.region.cpu_hz() {
            self.sample_phase -= self.region.cpu_hz();
            let mut value = 0.0;
            for (i, &coefficient) in crate::audio_filter::FIR.iter().enumerate() {
                value += coefficient * self.history[(self.history_pos + i) & 1023];
            }
            self.high_pass = 0.995 * (self.high_pass + value - self.previous);
            self.previous = value;
            self.low_pass += 0.65 * (self.high_pass - self.low_pass);
            if self.sample_count < 2048 {
                self.samples[self.sample_count] = self.low_pass;
                self.sample_count += 1;
            }
        }
    }
    pub fn dmc_request(&self) -> Option<u16> {
        if !self.dmc_full && self.dmc_remaining > 0 {
            Some(self.dmc_address)
        } else {
            None
        }
    }
    pub fn supply_dmc(&mut self, byte: u8) {
        self.dmc_buffer = byte;
        self.dmc_full = true;
        self.dmc_address = if self.dmc_address == 0xffff {
            0x8000
        } else {
            self.dmc_address + 1
        };
        self.dmc_remaining -= 1;
        if self.dmc_remaining == 0 {
            if self.regs[0x10] & 0x40 != 0 {
                self.restart_dmc();
            } else if self.regs[0x10] & 0x80 != 0 {
                self.dmc_irq = true;
            }
        }
    }
    fn restart_dmc(&mut self) {
        self.dmc_address = 0xc000 | (self.regs[0x12] as u16 * 64);
        self.dmc_remaining = self.regs[0x13] as u16 * 16 + 1;
    }
    pub fn read_register(&mut self, addr: u16) -> u8 {
        if addr != 0x4015 {
            return 0;
        }
        let value = u8::from(self.pulse[0].length > 0)
            | (u8::from(self.pulse[1].length > 0) << 1)
            | (u8::from(self.triangle_length > 0) << 2)
            | (u8::from(self.noise_length > 0) << 3)
            | (u8::from(self.dmc_remaining > 0) << 4)
            | (u8::from(self.frame_irq) << 6)
            | (u8::from(self.dmc_irq) << 7);
        self.frame_irq = false;
        value
    }
    pub fn write_register(&mut self, addr: u16, val: u8) {
        if !(0x4000..=0x4017).contains(&addr) {
            return;
        }
        self.regs[(addr - 0x4000) as usize] = val;
        match addr {
            0x4000..=0x4007 => {
                let i = (addr as usize - 0x4000) / 4;
                let p = &mut self.pulse[i];
                match addr & 3 {
                    1 => p.sweep_reload = true,
                    2 => p.period = (p.period & 0x700) | val as u16,
                    3 => {
                        p.period = (p.period & 0xff) | ((val as u16 & 7) << 8);
                        p.sequence = 0;
                        p.envelope.start = true;
                        if self.regs[0x15] & (1 << i) != 0 {
                            p.length = LENGTH[(val >> 3) as usize];
                        }
                    }
                    _ => {}
                }
            }
            0x400a => self.triangle_period = (self.triangle_period & 0x700) | val as u16,
            0x400b => {
                self.triangle_period = (self.triangle_period & 0xff) | ((val as u16 & 7) << 8);
                self.linear_reload = true;
                if self.regs[0x15] & 4 != 0 {
                    self.triangle_length = LENGTH[(val >> 3) as usize];
                }
            }
            0x400f => {
                self.noise_envelope.start = true;
                if self.regs[0x15] & 8 != 0 {
                    self.noise_length = LENGTH[(val >> 3) as usize];
                }
            }
            0x4010 => {
                if val & 0x80 == 0 {
                    self.dmc_irq = false;
                }
            }
            0x4011 => self.dmc_output = val & 0x7f,
            0x4015 => {
                for i in 0..2 {
                    if val & (1 << i) == 0 {
                        self.pulse[i].length = 0;
                    }
                }
                if val & 4 == 0 {
                    self.triangle_length = 0;
                }
                if val & 8 == 0 {
                    self.noise_length = 0;
                }
                if val & 0x10 == 0 {
                    self.dmc_remaining = 0;
                } else if self.dmc_remaining == 0 {
                    self.restart_dmc();
                }
                self.dmc_irq = false;
            }
            0x4017 => {
                self.five_step = val & 0x80 != 0;
                self.irq_inhibit = val & 0x40 != 0;
                if self.irq_inhibit {
                    self.frame_irq = false;
                }
                self.reset_delay = if self.total & 1 == 0 { 3 } else { 4 };
            }
            _ => {}
        }
    }
    pub(crate) fn valid_state(&self) -> bool {
        self.cycle <= if self.region == Region::Pal { 41566 } else { 37282 }
            && self.dmc_output <= 127
            && self.sample_count <= 2048
            && self.sample_phase < self.region.cpu_hz()
            && self.history_pos < 1024
            && self.dmc_bits > 0
            && self.dmc_bits <= 8
            && self.triangle_sequence < 32
            && self
                .pulse
                .iter()
                .all(|p| p.sequence < 8 && p.period <= 0x7ff)
            && self.samples.iter().all(|s| s.is_finite())
            && self.history.iter().all(|s| s.is_finite())
            && self.previous.is_finite()
            && self.high_pass.is_finite()
            && self.low_pass.is_finite()
    }
}
crate::state::state_fields!(Envelope, start, divider, decay);
crate::state::state_fields!(
    Pulse,
    timer,
    period,
    sequence,
    length,
    envelope,
    sweep_divider,
    sweep_reload
);
crate::state::state_fields!(
    Apu,
    regs,
    five_step,
    irq_inhibit,
    frame_irq,
    cycle,
    total,
    reset_delay,
    pulse,
    triangle_timer,
    triangle_period,
    triangle_sequence,
    triangle_length,
    linear,
    linear_reload,
    noise_timer,
    noise_shift,
    noise_length,
    noise_envelope,
    dmc_timer,
    dmc_address,
    dmc_remaining,
    dmc_buffer,
    dmc_full,
    dmc_shift,
    dmc_bits,
    dmc_silent,
    dmc_output,
    dmc_irq,
    sample_phase,
    history,
    history_pos,
    previous,
    high_pass,
    low_pass,
    samples,
    sample_count;
    region: Region::Ntsc
);
