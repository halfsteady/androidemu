//! Standard NES controller: an 8-bit parallel-to-serial shift register.
//!
//! Strobing high continuously reloads the register from the button state; strobing
//! low latches it, after which each read of $4016/$4017 shifts out one bit, A first.

#[derive(Clone, Copy, Default)]
pub struct Buttons(pub u8);

impl Buttons {
    pub const A: u8 = 1 << 0;
    pub const B: u8 = 1 << 1;
    pub const SELECT: u8 = 1 << 2;
    pub const START: u8 = 1 << 3;
    pub const UP: u8 = 1 << 4;
    pub const DOWN: u8 = 1 << 5;
    pub const LEFT: u8 = 1 << 6;
    pub const RIGHT: u8 = 1 << 7;
}

#[derive(Clone, Default)]
pub struct Controller {
    state: u8,
    shift: u8,
    strobe: bool,
}

impl Controller {
    pub fn set_buttons(&mut self, b: Buttons) {
        self.state = b.0;
        if self.strobe {
            self.shift = self.state;
        }
    }

    /// Untimed convenience for controller-only callers. The console bus uses
    /// the phase-clocked line/latch methods below.
    pub fn write_strobe(&mut self, val: u8) {
        self.set_strobe_line(val);
        self.clock_strobe();
    }

    /// The console updates the line on writes, then latches it on put cycles.
    pub(crate) fn set_strobe_line(&mut self, val: u8) {
        self.strobe = val & 1 != 0;
    }

    pub(crate) fn clock_strobe(&mut self) {
        if self.strobe {
            self.shift = self.state;
        }
    }

    pub fn read(&mut self) -> u8 {
        if self.strobe {
            // While strobing, reads return A repeatedly without shifting.
            return self.state & 1;
        }
        let bit = self.shift & 1;
        // After eight reads the register shifts in 1s, which is why the ninth read
        // and beyond return $01 on an official controller.
        self.shift = (self.shift >> 1) | 0x80;
        bit
    }
}

crate::state::state_fields!(Controller, state, shift, strobe);
