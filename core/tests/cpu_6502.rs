//! 6502 conformance tests that need no external ROM.
//!
//! These cover the things that are easy to get subtly wrong and that a test ROM
//! would only tell you about as a single pass/fail: exact cycle counts per
//! addressing mode, the page-cross penalty rules, dummy reads and writes, the
//! interrupt entry sequences, and the ALU flag edge cases.

use nes_core::cpu::{flags, Bus, Cpu};

/// A flat 64 KB address space, with every access logged.
struct FlatBus {
    mem: Vec<u8>,
    log: Vec<Access>,
    nmi: bool,
    irq: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Access {
    Read(u16),
    Write(u16, u8),
}

impl FlatBus {
    fn new() -> FlatBus {
        FlatBus { mem: vec![0; 0x10000], log: Vec::new(), nmi: false, irq: false }
    }

    fn load(&mut self, addr: u16, bytes: &[u8]) {
        self.mem[addr as usize..addr as usize + bytes.len()].copy_from_slice(bytes);
    }

    fn writes(&self) -> Vec<(u16, u8)> {
        self.log
            .iter()
            .filter_map(|a| match a {
                Access::Write(addr, v) => Some((*addr, *v)),
                _ => None,
            })
            .collect()
    }

    fn reads(&self) -> Vec<u16> {
        self.log
            .iter()
            .filter_map(|a| match a {
                Access::Read(addr) => Some(*addr),
                _ => None,
            })
            .collect()
    }
}

impl Bus for FlatBus {
    fn read(&mut self, addr: u16) -> u8 {
        self.log.push(Access::Read(addr));
        self.mem[addr as usize]
    }
    fn write(&mut self, addr: u16, val: u8) {
        self.log.push(Access::Write(addr, val));
        self.mem[addr as usize] = val;
    }
    fn nmi_line(&self) -> bool {
        self.nmi
    }
    fn irq_line(&self) -> bool {
        self.irq
    }
}

/// Assemble `code` at $0200, run one instruction, and return (cpu, bus).
fn run(code: &[u8], setup: impl FnOnce(&mut Cpu, &mut FlatBus)) -> (Cpu, FlatBus, u64) {
    let mut bus = FlatBus::new();
    let mut cpu = Cpu::new();
    bus.load(0x0200, code);
    cpu.set_pc(0x0200);
    setup(&mut cpu, &mut bus);
    bus.log.clear();
    let cycles = cpu.step(&mut bus);
    (cpu, bus, cycles)
}

// ---------------------------------------------------------------- cycle counts

/// Every entry is (name, code, setup, expected cycles). These are the canonical
/// 6502 timings; if the emergent count from bus accesses disagrees with one of
/// them, the addressing mode is performing the wrong number of accesses.
#[test]
fn cycle_counts_match_hardware() {
    #[allow(clippy::type_complexity)]
    let cases: Vec<(&str, Vec<u8>, Box<dyn Fn(&mut Cpu, &mut FlatBus)>, u64)> = vec![
        ("LDA #", vec![0xA9, 0x42], Box::new(|_: &mut Cpu, _: &mut FlatBus| {}), 2),
        ("LDA zp", vec![0xA5, 0x10], Box::new(|_: &mut Cpu, _: &mut FlatBus| {}), 3),
        ("LDA zp,X", vec![0xB5, 0x10], Box::new(|c: &mut Cpu, _: &mut FlatBus| c.x = 5), 4),
        ("LDA abs", vec![0xAD, 0x00, 0x03], Box::new(|_: &mut Cpu, _: &mut FlatBus| {}), 4),
        ("LDA abs,X no cross", vec![0xBD, 0x00, 0x03], Box::new(|c: &mut Cpu, _: &mut FlatBus| c.x = 0x10), 4),
        ("LDA abs,X cross", vec![0xBD, 0xF0, 0x03], Box::new(|c: &mut Cpu, _: &mut FlatBus| c.x = 0x20), 5),
        ("LDA abs,Y no cross", vec![0xB9, 0x00, 0x03], Box::new(|c: &mut Cpu, _: &mut FlatBus| c.y = 0x10), 4),
        ("LDA abs,Y cross", vec![0xB9, 0xF0, 0x03], Box::new(|c: &mut Cpu, _: &mut FlatBus| c.y = 0x20), 5),
        ("LDA (ind,X)", vec![0xA1, 0x10], Box::new(|c: &mut Cpu, _: &mut FlatBus| c.x = 4), 6),
        ("LDA (ind),Y no cross", vec![0xB1, 0x10],
            Box::new(|c: &mut Cpu, b: &mut FlatBus| { c.y = 1; b.load(0x10, &[0x00, 0x03]); }), 5),
        ("LDA (ind),Y cross", vec![0xB1, 0x10],
            Box::new(|c: &mut Cpu, b: &mut FlatBus| { c.y = 0xFF; b.load(0x10, &[0x01, 0x03]); }), 6),
        // Stores always pay the indexing penalty, crossing or not.
        ("STA abs,X no cross", vec![0x9D, 0x00, 0x03], Box::new(|c: &mut Cpu, _: &mut FlatBus| c.x = 0x10), 5),
        ("STA abs,X cross", vec![0x9D, 0xF0, 0x03], Box::new(|c: &mut Cpu, _: &mut FlatBus| c.x = 0x20), 5),
        ("STA (ind),Y no cross", vec![0x91, 0x10],
            Box::new(|c: &mut Cpu, b: &mut FlatBus| { c.y = 1; b.load(0x10, &[0x00, 0x03]); }), 6),
        // Read-modify-write.
        ("INC zp", vec![0xE6, 0x10], Box::new(|_: &mut Cpu, _: &mut FlatBus| {}), 5),
        ("INC zp,X", vec![0xF6, 0x10], Box::new(|c: &mut Cpu, _: &mut FlatBus| c.x = 1), 6),
        ("INC abs", vec![0xEE, 0x00, 0x03], Box::new(|_: &mut Cpu, _: &mut FlatBus| {}), 6),
        ("INC abs,X", vec![0xFE, 0x00, 0x03], Box::new(|c: &mut Cpu, _: &mut FlatBus| c.x = 1), 7),
        ("ASL A", vec![0x0A], Box::new(|_: &mut Cpu, _: &mut FlatBus| {}), 2),
        // Control flow and stack.
        ("JMP abs", vec![0x4C, 0x00, 0x03], Box::new(|_: &mut Cpu, _: &mut FlatBus| {}), 3),
        ("JMP (ind)", vec![0x6C, 0x00, 0x03], Box::new(|_: &mut Cpu, _: &mut FlatBus| {}), 5),
        ("JSR", vec![0x20, 0x00, 0x03], Box::new(|_: &mut Cpu, _: &mut FlatBus| {}), 6),
        ("RTS", vec![0x60], Box::new(|_: &mut Cpu, _: &mut FlatBus| {}), 6),
        ("RTI", vec![0x40], Box::new(|_: &mut Cpu, _: &mut FlatBus| {}), 6),
        ("BRK", vec![0x00], Box::new(|_: &mut Cpu, _: &mut FlatBus| {}), 7),
        ("PHA", vec![0x48], Box::new(|_: &mut Cpu, _: &mut FlatBus| {}), 3),
        ("PHP", vec![0x08], Box::new(|_: &mut Cpu, _: &mut FlatBus| {}), 3),
        ("PLA", vec![0x68], Box::new(|_: &mut Cpu, _: &mut FlatBus| {}), 4),
        ("PLP", vec![0x28], Box::new(|_: &mut Cpu, _: &mut FlatBus| {}), 4),
        ("NOP", vec![0xEA], Box::new(|_: &mut Cpu, _: &mut FlatBus| {}), 2),
        ("TAX", vec![0xAA], Box::new(|_: &mut Cpu, _: &mut FlatBus| {}), 2),
        // Branches: not taken, taken, taken across a page.
        ("BEQ not taken", vec![0xF0, 0x10], Box::new(|c: &mut Cpu, _: &mut FlatBus| c.p &= !flags::Z), 2),
        ("BEQ taken", vec![0xF0, 0x10], Box::new(|c: &mut Cpu, _: &mut FlatBus| c.p |= flags::Z), 3),
    ];

    let mut failures = Vec::new();
    for (name, code, setup, expected) in cases {
        let (_, _, cycles) = run(&code, |c, b| setup(c, b));
        if cycles != expected {
            failures.push(format!("{name}: expected {expected} cycles, got {cycles}"));
        }
    }
    assert!(failures.is_empty(), "cycle count mismatches:\n  {}", failures.join("\n  "));
}

#[test]
fn branch_across_page_costs_an_extra_cycle() {
    // Place the branch so the target lands in the next page.
    let mut bus = FlatBus::new();
    let mut cpu = Cpu::new();
    bus.load(0x02F0, &[0xF0, 0x20]); // BEQ +$20 -> $0312, crossing into page 3
    cpu.set_pc(0x02F0);
    cpu.p |= flags::Z;
    let cycles = cpu.step(&mut bus);
    assert_eq!(cycles, 4, "taken branch across a page boundary is 4 cycles");
    assert_eq!(cpu.pc, 0x0312);
}

// ------------------------------------------------------------- dummy accesses

#[test]
fn rmw_writes_the_old_value_before_the_new_one() {
    // Boards with write-triggered registers can see this dummy write, so it is
    // part of the contract, not an implementation detail.
    let (_, bus, _) = run(&[0xE6, 0x10], |_, b| b.load(0x10, &[0x41]));
    assert_eq!(
        bus.writes(),
        vec![(0x0010, 0x41), (0x0010, 0x42)],
        "INC should dummy-write the unmodified value first"
    );
}

#[test]
fn indexed_dummy_read_uses_the_uncarried_address() {
    // LDA $03F0,X with X=$20 targets $0410, but the dummy read hits $0310 - the
    // address on the bus before the carry propagated into the high byte.
    let (_, bus, _) = run(&[0xBD, 0xF0, 0x03], |c, _| c.x = 0x20);
    assert!(
        bus.reads().contains(&0x0310),
        "expected a dummy read at $0310, got {:04X?}",
        bus.reads()
    );
}

#[test]
fn zero_page_indexed_wraps_within_the_page() {
    let (cpu, _, _) = run(&[0xB5, 0xF0], |c, b| {
        c.x = 0x20; // $F0 + $20 = $110, which wraps to $10
        b.load(0x10, &[0x99]);
    });
    assert_eq!(cpu.a, 0x99);
}

#[test]
fn indirect_x_wraps_within_zero_page() {
    let (cpu, _, _) = run(&[0xA1, 0xFF], |c, b| {
        c.x = 0x00;
        b.load(0xFF, &[0x34]); // low byte at $FF...
        b.load(0x00, &[0x12]); // ...high byte wraps to $00, not $0100
        b.load(0x1234, &[0x7E]);
    });
    assert_eq!(cpu.a, 0x7E);
}

#[test]
fn jmp_indirect_reproduces_the_page_wrap_bug() {
    // JMP ($03FF) reads its high byte from $0300, not $0400.
    let (cpu, _, _) = run(&[0x6C, 0xFF, 0x03], |_, b| {
        b.load(0x03FF, &[0x21]);
        b.load(0x0300, &[0x43]);
        b.load(0x0400, &[0xFF]); // would be read by a "fixed" 6502
    });
    assert_eq!(cpu.pc, 0x4321, "the indirect JMP page-wrap bug is not reproduced");
}

// ------------------------------------------------------------------ ALU flags

#[test]
fn adc_sets_overflow_only_on_signed_overflow() {
    // (operand, carry in, A) -> (result, C, V)
    let cases: [(u8, u8, bool, u8, bool, bool); 6] = [
        // 0x50 + 0x10 = 0x60: no carry, no overflow
        (0x50, 0x10, false, 0x60, false, false),
        // 0x50 + 0x50 = 0xA0: positive + positive = negative -> overflow
        (0x50, 0x50, false, 0xA0, false, true),
        // 0x50 + 0x90 = 0xE0: mixed signs, never overflow
        (0x50, 0x90, false, 0xE0, false, false),
        // 0x50 + 0xD0 = 0x120: carry out, no overflow
        (0x50, 0xD0, false, 0x20, true, false),
        // 0xD0 + 0x90 = 0x160: negative + negative = positive -> overflow
        (0xD0, 0x90, false, 0x60, true, true),
        // carry in participates
        (0x00, 0xFF, true, 0x00, true, false),
    ];
    for (a, m, cin, want, want_c, want_v) in cases {
        let (cpu, _, _) = run(&[0x69, m], |c, _| {
            c.a = a;
            if cin {
                c.p |= flags::C;
            }
        });
        assert_eq!(cpu.a, want, "ADC {a:02X}+{m:02X}+{}", cin as u8);
        assert_eq!(cpu.p & flags::C != 0, want_c, "carry for {a:02X}+{m:02X}");
        assert_eq!(cpu.p & flags::V != 0, want_v, "overflow for {a:02X}+{m:02X}");
    }
}

#[test]
fn sbc_is_adc_with_the_operand_inverted() {
    // 0x50 - 0xB0 = 0xA0 with borrow; signed 80 - (-80) overflows.
    let (cpu, _, _) = run(&[0xE9, 0xB0], |c, _| {
        c.a = 0x50;
        c.p |= flags::C; // no borrow in
    });
    assert_eq!(cpu.a, 0xA0);
    assert!(cpu.p & flags::C == 0, "borrow should clear carry");
    assert!(cpu.p & flags::V != 0, "signed overflow expected");
}

#[test]
fn decimal_flag_does_not_affect_arithmetic_on_the_2a03() {
    // The 2A03 has its BCD circuitry disconnected. 0x09 + 0x01 stays 0x0A.
    let (cpu, _, _) = run(&[0x69, 0x01], |c, _| {
        c.a = 0x09;
        c.p |= flags::D;
    });
    assert_eq!(cpu.a, 0x0A, "decimal mode must be inert");
}

#[test]
fn compare_sets_carry_when_register_is_greater_or_equal() {
    for (reg, operand, want_c, want_z) in
        [(0x10u8, 0x10u8, true, true), (0x10, 0x0F, true, false), (0x0F, 0x10, false, false)]
    {
        let (cpu, _, _) = run(&[0xC9, operand], |c, _| c.a = reg);
        assert_eq!(cpu.p & flags::C != 0, want_c, "CMP {reg:02X} vs {operand:02X} carry");
        assert_eq!(cpu.p & flags::Z != 0, want_z, "CMP {reg:02X} vs {operand:02X} zero");
    }
}

#[test]
fn bit_takes_n_and_v_from_memory_not_from_the_and() {
    let (cpu, _, _) = run(&[0x24, 0x10], |c, b| {
        c.a = 0x00; // AND result is zero...
        b.load(0x10, &[0xC0]); // ...but bits 7 and 6 of memory are set
    });
    assert!(cpu.p & flags::Z != 0, "Z from the AND result");
    assert!(cpu.p & flags::N != 0, "N from memory bit 7");
    assert!(cpu.p & flags::V != 0, "V from memory bit 6");
}

// -------------------------------------------------------------------- stack

#[test]
fn php_pushes_b_and_u_set() {
    let (_, bus, _) = run(&[0x08], |c, _| c.p = flags::C);
    let pushed = bus.writes()[0].1;
    assert_eq!(pushed, flags::C | flags::B | flags::U, "PHP sets B and U in the pushed byte");
}

#[test]
fn plp_ignores_b_and_forces_u() {
    let (cpu, _, _) = run(&[0x28], |c, b| {
        c.s = 0xFC;
        b.load(0x01FD, &[0x00]); // pull a byte with B and U clear
    });
    assert!(cpu.p & flags::U != 0, "U always reads as set");
    assert!(cpu.p & flags::B == 0, "B is not a real register bit");
}

#[test]
fn jsr_pushes_the_address_of_its_last_byte() {
    // JSR at $0200 occupies $0200-$0202; the pushed return address is $0202,
    // and RTS adds one to get $0203.
    let (cpu, bus, _) = run(&[0x20, 0x00, 0x03], |_, _| {});
    assert_eq!(cpu.pc, 0x0300);
    let w = bus.writes();
    assert_eq!(w.len(), 2);
    assert_eq!(w[0].1, 0x02, "high byte of $0202");
    assert_eq!(w[1].1, 0x02, "low byte of $0202");
}

#[test]
fn jsr_rts_round_trips() {
    let mut bus = FlatBus::new();
    let mut cpu = Cpu::new();
    bus.load(0x0200, &[0x20, 0x00, 0x03]); // JSR $0300
    bus.load(0x0300, &[0x60]); // RTS
    cpu.set_pc(0x0200);
    cpu.step(&mut bus);
    assert_eq!(cpu.pc, 0x0300);
    cpu.step(&mut bus);
    assert_eq!(cpu.pc, 0x0203, "RTS should return past the JSR");
}

// --------------------------------------------------------------- interrupts

#[test]
fn brk_pushes_pc_plus_two_and_sets_b() {
    let (cpu, bus, _) = run(&[0x00, 0xEA], |c, b| {
        c.p = flags::C;
        b.load(0xFFFE, &[0x34, 0x12]);
    });
    let w = bus.writes();
    assert_eq!(w[0].1, 0x02, "PCH of $0202");
    assert_eq!(w[1].1, 0x02, "PCL of $0202 - BRK skips its signature byte");
    assert_eq!(w[2].1, flags::C | flags::B | flags::U, "BRK pushes B set");
    assert_eq!(cpu.pc, 0x1234);
    assert!(cpu.p & flags::I != 0, "BRK sets the interrupt disable");
}

#[test]
fn hardware_interrupt_pushes_b_clear() {
    let mut bus = FlatBus::new();
    let mut cpu = Cpu::new();
    bus.load(0x0200, &[0xEA, 0xEA, 0xEA, 0xEA]);
    bus.load(0xFFFA, &[0x34, 0x12]);
    cpu.set_pc(0x0200);
    // The NMI is edge-triggered, so the line has to transition from low to high
    // while the CPU is sampling. Run one instruction with it low, then raise it.
    cpu.step(&mut bus);
    bus.nmi = true;
    bus.log.clear();
    let cycles = cpu.step(&mut bus);
    assert_eq!(cycles, 7, "interrupt entry is 7 cycles");
    assert_eq!(cpu.pc, 0x1234, "vectored through $FFFA");
    let pushed = bus.writes()[2].1;
    assert_eq!(pushed & flags::B, 0, "a hardware interrupt pushes B clear");
}

#[test]
fn nmi_is_edge_triggered_not_level_triggered() {
    let mut bus = FlatBus::new();
    let mut cpu = Cpu::new();
    bus.load(0x0200, &[0xEA, 0xEA, 0xEA, 0xEA]);
    bus.load(0xFFFA, &[0x00, 0x03]);
    bus.load(0x0300, &[0xEA, 0xEA, 0xEA]);
    cpu.set_pc(0x0200);
    cpu.step(&mut bus);
    bus.nmi = true;
    cpu.step(&mut bus); // services the NMI
    assert_eq!(cpu.pc, 0x0300, "NMI should vector to $0300");
    // The line is still high, but with no new edge there must be no second NMI.
    let before = cpu.pc;
    cpu.step(&mut bus);
    assert_eq!(cpu.pc, before.wrapping_add(1), "a held NMI line must not re-fire");
}

#[test]
fn irq_is_masked_by_the_i_flag() {
    let mut bus = FlatBus::new();
    let mut cpu = Cpu::new();
    bus.load(0x0200, &[0xEA, 0xEA, 0xEA, 0xEA]);
    bus.load(0xFFFE, &[0x00, 0x04]);
    bus.load(0x0400, &[0xEA, 0xEA]);
    cpu.set_pc(0x0200);
    cpu.p |= flags::I;
    bus.irq = true;
    cpu.step(&mut bus);
    assert_ne!(cpu.pc, 0x0400, "IRQ must be masked while I is set");

    // CLI changes I after interrupt polling, so one more instruction runs.
    bus.mem[cpu.pc as usize] = 0x58;
    cpu.step(&mut bus);
    assert_eq!(cpu.p & flags::I, 0);
    cpu.step(&mut bus);
    assert_ne!(cpu.pc, 0x0400, "CLI must delay IRQ through the next instruction");
    cpu.step(&mut bus);
    assert_eq!(cpu.pc, 0x0400, "IRQ should fire after CLI's delay");
}

#[test]
fn reset_burns_seven_cycles_and_reads_the_vector() {
    let mut bus = FlatBus::new();
    let mut cpu = Cpu::new();
    bus.load(0xFFFC, &[0x00, 0xC0]);
    cpu.reset(&mut bus);
    assert_eq!(cpu.pc, 0xC000);
    assert_eq!(cpu.cycles, 7, "reset takes 7 cycles");
    assert_eq!(cpu.s, 0xFA, "reset decrements S three times from $FD");
}

// ------------------------------------------------------------- undocumented

#[test]
fn undocumented_ops_behave_as_documented_combinations() {
    // LAX loads both A and X.
    let (cpu, _, _) = run(&[0xA7, 0x10], |_, b| b.load(0x10, &[0x5A]));
    assert_eq!((cpu.a, cpu.x), (0x5A, 0x5A), "LAX");

    // SAX stores A & X without touching flags.
    let (_, bus, _) = run(&[0x87, 0x10], |c, _| {
        c.a = 0xF0;
        c.x = 0x3C;
    });
    assert_eq!(bus.writes(), vec![(0x0010, 0x30)], "SAX stores A & X");

    // DCP decrements memory then compares against A.
    let (cpu, bus, _) = run(&[0xC7, 0x10], |c, b| {
        c.a = 0x40;
        b.load(0x10, &[0x41]);
    });
    assert_eq!(bus.writes().last(), Some(&(0x0010, 0x40)), "DCP decrements");
    assert!(cpu.p & flags::Z != 0, "DCP compares the decremented value against A");

    // SLO shifts memory left then ORs into A.
    let (cpu, _, _) = run(&[0x07, 0x10], |c, b| {
        c.a = 0x01;
        b.load(0x10, &[0x40]);
    });
    assert_eq!(cpu.a, 0x81, "SLO = ASL then ORA");

    // ANC copies bit 7 of the result into carry.
    let (cpu, _, _) = run(&[0x0B, 0xFF], |c, _| c.a = 0x80);
    assert!(cpu.p & flags::C != 0, "ANC sets carry from bit 7");
}

#[test]
fn kil_jams_the_processor() {
    let mut bus = FlatBus::new();
    let mut cpu = Cpu::new();
    bus.load(0x0200, &[0x02]);
    cpu.set_pc(0x0200);
    cpu.step(&mut bus);
    assert!(cpu.jammed, "KIL should jam");
    let pc = cpu.pc;
    cpu.step(&mut bus);
    assert_eq!(cpu.pc, pc, "a jammed CPU makes no progress");
}
