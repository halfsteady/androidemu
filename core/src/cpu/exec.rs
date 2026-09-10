//! Instruction execution. Split from `mod.rs` only for length; this is the second
//! half of the `Cpu` implementation.

use super::table::{Access, Mode, Op, OPS};
use super::{flags, page_crossed, Bus, Cpu, IRQ_VECTOR, STACK_BASE};

impl Cpu {
    pub(super) fn execute<B: Bus>(&mut self, bus: &mut B, opcode: u8) {
        let entry = OPS[opcode as usize];
        let (op, mode, access) = (entry.op, entry.mode, entry.access);

        // Instructions whose cycle pattern does not fit the generic
        // resolve-then-access shape are handled before addressing runs.
        match op {
            Op::Jsr => return self.op_jsr(bus),
            Op::Brk => return self.op_brk(bus),
            Op::Rts => return self.op_rts(bus),
            Op::Rti => return self.op_rti(bus),
            Op::Pha | Op::Php | Op::Pla | Op::Plp => return self.op_stack(bus, op),
            Op::Kil => {
                self.jammed = true;
                self.pc = self.pc.wrapping_sub(1);
                return;
            }
            _ => {}
        }

        let addr = self.resolve(bus, mode, access);

        match access {
            Access::Read => {
                let v = self.read(bus, addr);
                self.apply_read(op, v);
            }
            Access::Write => {
                let unstable = matches!(op, Op::Ahx | Op::Shx | Op::Shy | Op::Tas);
                let base = if unstable {
                    addr.wrapping_sub(if mode == Mode::AbsX { self.x } else { self.y } as u16)
                } else { addr };
                let high_mask = ((base >> 8) as u8).wrapping_add(1);
                let mask = if unstable && bus.read_halted() {
                    0xff
                } else {
                    high_mask
                };
                let v = self.write_value(op, mask);
                let target = if unstable && page_crossed(base, addr) {
                    (((v & high_mask) as u16) << 8) | (addr & 0xff)
                } else { addr };
                self.write(bus, target, v);
            }
            Access::Rmw => {
                let old = self.read(bus, addr);
                // The dummy write-back of the unmodified value. Boards with
                // write-triggered registers see this, so it is not optional.
                self.write(bus, addr, old);
                let new = self.apply_rmw(op, old);
                self.write(bus, addr, new);
            }
            Access::None => self.apply_none(bus, op, mode, addr),
        }
    }

    // ---- reads ----

    fn apply_read(&mut self, op: Op, v: u8) {
        match op {
            Op::Lda => {
                self.a = v;
                self.set_zn(v);
            }
            Op::Ldx => {
                self.x = v;
                self.set_zn(v);
            }
            Op::Ldy => {
                self.y = v;
                self.set_zn(v);
            }
            Op::Eor => {
                self.a ^= v;
                let a = self.a;
                self.set_zn(a);
            }
            Op::And => {
                self.a &= v;
                let a = self.a;
                self.set_zn(a);
            }
            Op::Ora => {
                self.a |= v;
                let a = self.a;
                self.set_zn(a);
            }
            Op::Adc => self.adc(v),
            Op::Sbc => self.adc(!v),
            Op::Cmp => self.compare(self.a, v),
            Op::Cpx => self.compare(self.x, v),
            Op::Cpy => self.compare(self.y, v),
            Op::Bit => {
                self.set_flag(flags::Z, self.a & v == 0);
                self.set_flag(flags::V, v & 0x40 != 0);
                self.set_flag(flags::N, v & 0x80 != 0);
            }
            Op::Nop => {}

            // -- undocumented --
            Op::Lax => {
                self.a = v;
                self.x = v;
                self.set_zn(v);
            }
            Op::Anc => {
                self.a &= v;
                let a = self.a;
                self.set_zn(a);
                self.set_flag(flags::C, a & 0x80 != 0);
            }
            Op::Alr => {
                self.a &= v;
                let a = self.a;
                self.set_flag(flags::C, a & 1 != 0);
                self.a = a >> 1;
                let r = self.a;
                self.set_zn(r);
            }
            Op::Arr => {
                self.a &= v;
                let carry_in = if self.flag(flags::C) { 0x80 } else { 0 };
                self.a = (self.a >> 1) | carry_in;
                let a = self.a;
                self.set_zn(a);
                // ARR sets C and V from the *result*, not the input - the ALU's
                // adder is still wired in during the rotate.
                self.set_flag(flags::C, a & 0x40 != 0);
                self.set_flag(flags::V, ((a >> 6) ^ (a >> 5)) & 1 != 0);
            }
            Op::Axs => {
                let t = (self.a & self.x) as u16;
                let r = t.wrapping_sub(v as u16);
                self.set_flag(flags::C, t >= v as u16);
                self.x = r as u8;
                let x = self.x;
                self.set_zn(x);
            }
            Op::Xaa => {
                // Genuinely unstable on hardware; this is the behaviour most
                // documented boards exhibit and what test ROMs expect.
                self.a = self.x & v;
                let a = self.a;
                self.set_zn(a);
            }
            Op::Las => {
                let r = v & self.s;
                self.a = r;
                self.x = r;
                self.s = r;
                self.set_zn(r);
            }
            other => unreachable!("{:?} is not a read-access op", other),
        }
    }

    // ---- writes ----

    fn write_value(&mut self, op: Op, hi_plus_1: u8) -> u8 {
        // RDY going low during the indexing dummy read removes the high-byte
        // mask from AHX/SHX/SHY/TAS. Otherwise it is the literal high byte + 1.
        match op {
            Op::Sta => self.a,
            Op::Stx => self.x,
            Op::Sty => self.y,
            Op::Sax => self.a & self.x,
            Op::Ahx => self.a & self.x & hi_plus_1,
            Op::Shy => self.y & hi_plus_1,
            Op::Shx => self.x & hi_plus_1,
            Op::Tas => {
                self.s = self.a & self.x;
                self.s & hi_plus_1
            }
            other => unreachable!("{:?} is not a write-access op", other),
        }
    }

    // ---- read-modify-write ----

    fn apply_rmw(&mut self, op: Op, v: u8) -> u8 {
        match op {
            Op::Asl => self.asl(v),
            Op::Lsr => self.lsr(v),
            Op::Rol => self.rol(v),
            Op::Ror => self.ror(v),
            Op::Inc => {
                let r = v.wrapping_add(1);
                self.set_zn(r);
                r
            }
            Op::Dec => {
                let r = v.wrapping_sub(1);
                self.set_zn(r);
                r
            }

            // -- undocumented: an official RMW followed by an official ALU op --
            Op::Slo => {
                let r = self.asl(v);
                self.a |= r;
                let a = self.a;
                self.set_zn(a);
                r
            }
            Op::Rla => {
                let r = self.rol(v);
                self.a &= r;
                let a = self.a;
                self.set_zn(a);
                r
            }
            Op::Sre => {
                let r = self.lsr(v);
                self.a ^= r;
                let a = self.a;
                self.set_zn(a);
                r
            }
            Op::Rra => {
                let r = self.ror(v);
                self.adc(r);
                r
            }
            Op::Dcp => {
                let r = v.wrapping_sub(1);
                self.compare(self.a, r);
                r
            }
            Op::Isb => {
                let r = v.wrapping_add(1);
                self.adc(!r);
                r
            }
            other => unreachable!("{:?} is not an RMW op", other),
        }
    }

    // ---- everything else ----

    fn apply_none<B: Bus>(&mut self, bus: &mut B, op: Op, mode: Mode, addr: u16) {
        match op {
            Op::Jmp => self.pc = addr,

            Op::Bpl => self.branch(bus, addr, !self.flag(flags::N)),
            Op::Bmi => self.branch(bus, addr, self.flag(flags::N)),
            Op::Bvc => self.branch(bus, addr, !self.flag(flags::V)),
            Op::Bvs => self.branch(bus, addr, self.flag(flags::V)),
            Op::Bcc => self.branch(bus, addr, !self.flag(flags::C)),
            Op::Bcs => self.branch(bus, addr, self.flag(flags::C)),
            Op::Bne => self.branch(bus, addr, !self.flag(flags::Z)),
            Op::Beq => self.branch(bus, addr, self.flag(flags::Z)),

            Op::Clc => self.set_flag(flags::C, false),
            Op::Sec => self.set_flag(flags::C, true),
            Op::Cli => self.set_flag(flags::I, false),
            Op::Sei => self.set_flag(flags::I, true),
            Op::Cld => self.set_flag(flags::D, false),
            Op::Sed => self.set_flag(flags::D, true),
            Op::Clv => self.set_flag(flags::V, false),

            Op::Tax => {
                self.x = self.a;
                let v = self.x;
                self.set_zn(v);
            }
            Op::Tay => {
                self.y = self.a;
                let v = self.y;
                self.set_zn(v);
            }
            Op::Txa => {
                self.a = self.x;
                let v = self.a;
                self.set_zn(v);
            }
            Op::Tya => {
                self.a = self.y;
                let v = self.a;
                self.set_zn(v);
            }
            Op::Tsx => {
                self.x = self.s;
                let v = self.x;
                self.set_zn(v);
            }
            // TXS is the one transfer that does not touch the flags.
            Op::Txs => self.s = self.x,

            Op::Inx => {
                self.x = self.x.wrapping_add(1);
                let v = self.x;
                self.set_zn(v);
            }
            Op::Iny => {
                self.y = self.y.wrapping_add(1);
                let v = self.y;
                self.set_zn(v);
            }
            Op::Dex => {
                self.x = self.x.wrapping_sub(1);
                let v = self.x;
                self.set_zn(v);
            }
            Op::Dey => {
                self.y = self.y.wrapping_sub(1);
                let v = self.y;
                self.set_zn(v);
            }

            Op::Asl | Op::Lsr | Op::Rol | Op::Ror => {
                debug_assert_eq!(mode, Mode::Acc);
                let v = self.a;
                self.a = match op {
                    Op::Asl => self.asl(v),
                    Op::Lsr => self.lsr(v),
                    Op::Rol => self.rol(v),
                    _ => self.ror(v),
                };
            }

            Op::Nop => {}
            other => unreachable!("{:?} is not a none-access op", other),
        }
    }

    // ---- ALU helpers ----

    /// Add with carry. The 2A03 has its decimal mode disconnected, so BCD is never
    /// applied even when the D flag is set.
    fn adc(&mut self, v: u8) {
        let a = self.a as u16;
        let m = v as u16;
        let c = if self.flag(flags::C) { 1u16 } else { 0 };
        let sum = a + m + c;
        let r = sum as u8;
        self.set_flag(flags::C, sum > 0xFF);
        // Overflow when both operands share a sign that differs from the result's.
        self.set_flag(flags::V, (!(a ^ m) & (a ^ sum) & 0x80) != 0);
        self.a = r;
        self.set_zn(r);
    }

    fn compare(&mut self, reg: u8, v: u8) {
        let r = reg.wrapping_sub(v);
        self.set_flag(flags::C, reg >= v);
        self.set_zn(r);
    }

    fn asl(&mut self, v: u8) -> u8 {
        self.set_flag(flags::C, v & 0x80 != 0);
        let r = v << 1;
        self.set_zn(r);
        r
    }

    fn lsr(&mut self, v: u8) -> u8 {
        self.set_flag(flags::C, v & 0x01 != 0);
        let r = v >> 1;
        self.set_zn(r);
        r
    }

    fn rol(&mut self, v: u8) -> u8 {
        let carry_in = if self.flag(flags::C) { 1 } else { 0 };
        self.set_flag(flags::C, v & 0x80 != 0);
        let r = (v << 1) | carry_in;
        self.set_zn(r);
        r
    }

    fn ror(&mut self, v: u8) -> u8 {
        let carry_in = if self.flag(flags::C) { 0x80 } else { 0 };
        self.set_flag(flags::C, v & 0x01 != 0);
        let r = (v >> 1) | carry_in;
        self.set_zn(r);
        r
    }

    // ---- irregular instructions ----

    /// JSR interleaves its stack pushes between the two operand fetches, which is
    /// why it cannot go through `resolve`.
    fn op_jsr<B: Bus>(&mut self, bus: &mut B) {
        let lo = self.fetch(bus) as u16;
        self.read(bus, STACK_BASE | self.s as u16); // internal, discarded
        // The pushed address is the last byte of JSR, not the next instruction.
        self.push(bus, (self.pc >> 8) as u8);
        self.push(bus, self.pc as u8);
        let hi = self.read(bus, self.pc) as u16;
        self.pc = lo | (hi << 8);
    }

    fn op_brk<B: Bus>(&mut self, bus: &mut B) {
        // BRK reads and discards the byte after the opcode - the "signature" byte.
        self.read(bus, self.pc);
        self.pc = self.pc.wrapping_add(1);
        self.interrupt(bus, IRQ_VECTOR, true);
    }

    fn op_rts<B: Bus>(&mut self, bus: &mut B) {
        self.read(bus, self.pc); // dummy
        self.read(bus, STACK_BASE | self.s as u16); // dummy stack read
        let lo = self.pull(bus) as u16;
        let hi = self.pull(bus) as u16;
        self.pc = lo | (hi << 8);
        self.read(bus, self.pc); // the cycle that increments PC
        self.pc = self.pc.wrapping_add(1);
    }

    fn op_rti<B: Bus>(&mut self, bus: &mut B) {
        self.read(bus, self.pc);
        self.read(bus, STACK_BASE | self.s as u16);
        let p = self.pull(bus);
        // B and U are not real bits; they come back as whatever the register holds.
        self.p = (p & !flags::B) | flags::U;
        let lo = self.pull(bus) as u16;
        let hi = self.pull(bus) as u16;
        self.pc = lo | (hi << 8);
    }

    fn op_stack<B: Bus>(&mut self, bus: &mut B, op: Op) {
        self.read(bus, self.pc); // dummy operand fetch
        match op {
            Op::Pha => self.push(bus, self.a),
            Op::Php => {
                let v = self.p | flags::B | flags::U;
                self.push(bus, v);
            }
            Op::Pla => {
                self.read(bus, STACK_BASE | self.s as u16); // pre-increment dummy
                self.a = self.pull_no_dummy(bus);
                let a = self.a;
                self.set_zn(a);
            }
            Op::Plp => {
                self.read(bus, STACK_BASE | self.s as u16);
                let p = self.pull_no_dummy(bus);
                self.p = (p & !flags::B) | flags::U;
            }
            _ => unreachable!(),
        }
    }

    fn pull_no_dummy<B: Bus>(&mut self, bus: &mut B) -> u8 {
        self.s = self.s.wrapping_add(1);
        let addr = STACK_BASE | self.s as u16;
        self.read(bus, addr)
    }
}

// Silence the unused-import warning when `page_crossed` is only used in mod.rs.
#[allow(unused)]
fn _keep(a: u16, b: u16) -> bool {
    page_crossed(a, b)
}
