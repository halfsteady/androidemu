mod common;

use nes_core::{
    cpu::{flags, Bus},
    ppu::Ppu,
    Cartridge, Cpu, NesBus,
};

fn bus() -> NesBus {
    NesBus::new(Cartridge::load(&common::rom(0, 0)).unwrap())
}

#[test]
fn dmc_drives_external_bus_without_replacing_the_internal_latch() {
    // The DMA byte and the CPU's last byte deliberately disagree on bit 5.
    for (last_cpu, sample) in [(0x20, 0x00), (0x00, 0x20)] {
        let mut rom = common::rom(0, 0);
        rom[16 + 0x4000] = sample; // NROM's $C000 DMC sample.
        let mut bus = NesBus::new(Cartridge::load(&rom).unwrap());
        bus.write(0, last_cpu);
        bus.apu.write_register(0x4012, 0);
        bus.apu.write_register(0x4013, 0);
        bus.apu.write_register(0x4015, 0x10);
        assert_eq!(bus.read(0x4015) & 0x20, last_cpu);
        assert!(bus.read_halted());
        assert_eq!(bus.read(0x4000), sample);
        assert!(!bus.read_halted());
    }
}

#[test]
fn dmc_waits_for_a_read_and_accounts_for_three_or_four_stolen_cycles() {
    for preceding_writes in 0..2 {
        let mut nes = nes_core::Nes::new(&common::rom(0, 0)).unwrap();
        nes.cpu.pc = 0x200;
        nes.bus.ram[0x200] = 0xea; // Two-cycle NOP.
        nes.bus.apu.write_register(0x4015, 0x10);
        for _ in 0..preceding_writes {
            nes.bus.write(0, 0);
            assert!(nes.bus.apu.dmc_request().is_some(), "a write cannot be halted");
        }
        let dot = nes.bus.ppu.dot;
        let cpu_cycle = nes.cpu.cycles;
        let elapsed = nes.step();
        assert_eq!(elapsed, if preceding_writes == 0 { 6 } else { 5 });
        assert_eq!(nes.cpu.cycles - cpu_cycle, elapsed);
        assert_eq!(nes.bus.ppu.dot - dot, (elapsed * 3) as u16);
        assert!(nes.bus.apu.dmc_request().is_none());
        assert_eq!(nes.step(), 7, "the following BRK must not inherit DMA cycles");
    }
}

#[test]
fn ppu_open_bus_decays_only_bits_that_have_not_been_driven_again() {
    for (register, expected) in [(0x2000, 0), (0x2002, 0xe0), (0x2004, 0xff), (0x2007, 0x3f)] {
        let mut bus = bus();
        let mapper = bus.cart.mapper.as_mut();
        let ppu = &mut bus.ppu;
        ppu.write_register(0x2002, 0xff, mapper);
        for _ in 0..341 * 262 * 10 { ppu.tick(mapper); }
        ppu.status = 0xe0;
        ppu.oam[0] = 0xff;
        ppu.v = 0x3f00;
        ppu.palette[0] = 0x3f;
        ppu.read_register(register, mapper);
        for _ in 0..341 * 262 * 11 { ppu.tick(mapper); }
        assert_eq!(ppu.read_register(0x2000, mapper), expected, "refresh via {register:04x}");
    }
}

#[test]
fn apu_status_preserves_external_bus_and_floats_bit_five() {
    let mut bus = bus();
    bus.write(0, 0xaf);
    assert_eq!(bus.read(0x4015), 0x20);
    assert_eq!(
        bus.read(0x4000),
        0xaf,
        "APU status must not replace the external latch"
    );
    bus.write(0x4015, 0x05);
    assert_eq!(
        bus.read(0x4000),
        0x05,
        "writes to APU status do drive the external bus"
    );
    assert_eq!(bus.read(0x4015), 0);
}

#[test]
fn one_cycle_controller_strobe_depends_on_apu_phase() {
    // Clock out all eight released buttons so the next bit is the trailing 1.
    let mut missed = bus();
    for _ in 0..8 {
        missed.read(0x4016);
    }
    missed.write(0x4016, 1); // This high pulse ends before the latch clock.
    missed.write(0x4016, 0);
    assert_eq!(missed.read(0x4016) & 1, 1);

    let mut latched = bus();
    for _ in 0..8 {
        latched.read(0x4016);
    }
    latched.read(0); // Shift the same pulse by one CPU cycle.
    latched.write(0x4016, 1);
    latched.write(0x4016, 0);
    assert_eq!(latched.read(0x4016) & 1, 0);
}

#[test]
fn sprite_fetch_resets_oam_address_only_while_rendering() {
    let mut bus = bus();
    for mask in [0, 8, 16] {
        for scanline in [0, 239, 240, 241, 261] {
            for dot in [255, 256, 257, 319, 320] {
                let mut ppu = Ppu::new();
                ppu.mask = mask;
                ppu.scanline = scanline;
                ppu.dot = dot;
                ppu.oam_addr = 5;
                ppu.tick(bus.cart.mapper.as_mut());
                let fetch = mask != 0
                    && (scanline < 240 || scanline == 261)
                    && (257..=320).contains(&(dot + 1));
                assert_eq!(ppu.oam_addr, if fetch { 0 } else { 5 });
            }
        }
    }
}

#[test]
fn oam_attribute_holes_and_rendering_accesses() {
    let mut bus = bus();
    let mapper = bus.cart.mapper.as_mut();
    let mut ppu = Ppu::new();
    ppu.oam_addr = 2;
    ppu.write_register(0x2004, 0xff, mapper);
    ppu.oam_addr = 2;
    assert_eq!(ppu.read_register(0x2004, mapper), 0xe3);
    ppu.mask = 8;
    ppu.dot = 32;
    ppu.oam_addr = 1;
    ppu.oam[1] = 0x5a;
    assert_eq!(ppu.read_register(0x2004, mapper), 0xff);
    ppu.write_register(0x2004, 0, mapper);
    assert_eq!(ppu.oam_addr, 5);
    assert_eq!(
        ppu.oam[1], 0x5a,
        "rendering writes must not alter primary OAM"
    );
    ppu.mask = 0;
    ppu.oam_addr = 1;
    assert_eq!(ppu.read_register(0x2004, mapper), 0x5a);
}

#[test]
fn rendering_oam_writes_preserve_the_byte_offset_and_wrap() {
    let mut bus = bus();
    let mapper = bus.cart.mapper.as_mut();
    for scanline in [0, 239, 261] {
        for mask in [8, 16, 24] {
            let mut ppu = Ppu::new();
            ppu.scanline = scanline;
            ppu.mask = mask;
            ppu.dot = 100;
            ppu.oam.fill(0x5a);
            for address in 0..=255u8 {
                ppu.oam_addr = address;
                ppu.write_register(0x2004, 0xa5, mapper);
                assert_eq!(ppu.oam_addr, address.wrapping_add(4));
                assert_eq!(ppu.oam[address as usize], 0x5a);
            }
        }
    }
    // Rendering enabled during blanking still allows ordinary OAM writes.
    for scanline in [240, 241, 260] {
        let mut ppu = Ppu::new();
        ppu.scanline = scanline;
        ppu.mask = 24;
        ppu.oam_addr = 255;
        ppu.write_register(0x2004, 0xa5, mapper);
        assert_eq!(ppu.oam_addr, 0);
        assert_eq!(ppu.oam[255], 0xa5);
    }
}

struct EdgeBus {
    memory: Vec<u8>,
    cycle: usize,
    edge: usize,
    irq: bool,
    irq_window: std::ops::Range<usize>,
}
impl Bus for EdgeBus {
    fn read(&mut self, addr: u16) -> u8 {
        self.cycle += 1;
        self.memory[addr as usize]
    }
    fn write(&mut self, addr: u16, value: u8) {
        self.cycle += 1;
        self.memory[addr as usize] = value;
    }
    fn nmi_line(&self) -> bool {
        self.cycle >= self.edge
    }
    fn irq_line(&self) -> bool {
        self.irq || self.irq_window.contains(&self.cycle)
    }
}
fn interrupt_machine(edge: usize) -> (Cpu, EdgeBus) {
    let mut bus = EdgeBus {
        memory: vec![0; 65536],
        cycle: 0,
        edge,
        irq: false,
        irq_window: 0..0,
    };
    bus.memory[0xfffa..0xfffc].copy_from_slice(&[0, 0xa0]);
    bus.memory[0xfffe..].copy_from_slice(&[0, 0x90]);
    bus.memory[0x9000] = 0xe8; // INX: observable first instruction of IRQ handler.
    bus.memory[0xa000] = 0xe8;
    let mut cpu = Cpu::new();
    cpu.pc = 0x8000; // BRK followed by its padding byte.
    (cpu, bus)
}

#[test]
fn taken_branches_poll_before_cycle_three_and_again_only_on_page_crossing() {
    // Rising IRQ on operand read: a non-crossing branch waits through INX;
    // a crossing branch recognizes it at the additional poll.
    for crossing in [false, true] {
        let (mut cpu, mut bus) = interrupt_machine(usize::MAX);
        let start = if crossing { 0x80f0 } else { 0x8000 };
        cpu.pc = start;
        cpu.p = flags::U;
        bus.memory[start as usize..start as usize + 2].copy_from_slice(&[0xd0, 0x20]);
        let target = start + 2 + 0x20;
        bus.memory[target as usize] = 0xe8;
        bus.irq_window = 2..usize::MAX;
        assert_eq!(cpu.step(&mut bus), if crossing { 4 } else { 3 });
        assert_eq!(cpu.pc, target);
        if !crossing {
            assert_eq!(cpu.step(&mut bus), 2);
            assert_eq!(cpu.x, 1);
        }
        assert_eq!(cpu.step(&mut bus), 7);
        assert_eq!(cpu.pc, 0x9000);
    }
}

#[test]
fn page_crossing_branch_preserves_an_irq_recognized_by_the_first_poll() {
    let (mut cpu, mut bus) = interrupt_machine(usize::MAX);
    cpu.pc = 0x80f0;
    cpu.p = flags::U;
    bus.memory[0x80f0..0x80f2].copy_from_slice(&[0xd0, 0x20]);
    bus.irq_window = 1..3; // Asserted on opcode fetch, cleared on dummy read.
    assert_eq!(cpu.step(&mut bus), 4);
    assert_eq!(cpu.pc, 0x8112);
    assert_eq!(cpu.step(&mut bus), 7);
    assert_eq!(cpu.pc, 0x9000);
    assert_eq!(&bus.memory[0x1fc..0x1fe], &[0x12, 0x81]);
}

#[test]
fn early_nmi_hijacks_brk_without_changing_its_stack_frame() {
    let (mut cpu, mut bus) = interrupt_machine(3);
    assert_eq!(cpu.step(&mut bus), 7);
    assert_eq!(cpu.pc, 0xa000);
    assert_eq!(&bus.memory[0x1fc..0x1fe], &[0x02, 0x80]);
    assert_ne!(bus.memory[0x1fb] & flags::B, 0);
}

#[test]
fn late_nmi_waits_for_first_irq_handler_instruction() {
    let (mut cpu, mut bus) = interrupt_machine(6);
    assert_eq!(cpu.step(&mut bus), 7);
    assert_eq!(cpu.pc, 0x9000);
    assert_eq!(cpu.step(&mut bus), 2);
    assert_eq!(cpu.x, 1);
    assert_eq!(cpu.step(&mut bus), 7);
    assert_eq!(cpu.pc, 0xa000);
}

#[test]
fn interrupt_hijack_window_preserves_irq_and_brk_stack_frames() {
    for hardware_irq in [false, true] {
        for edge in 1..=7 {
            let (mut cpu, mut bus) = interrupt_machine(edge);
            cpu.p &= !flags::I;
            bus.irq = hardware_irq;
            assert_eq!(cpu.step(&mut bus), 7);
            assert_eq!(
                cpu.pc,
                if edge <= 4 { 0xa000 } else { 0x9000 },
                "IRQ={hardware_irq}, NMI edge on cycle {edge}"
            );
            assert_eq!(bus.memory[0x1fc], if hardware_irq { 0 } else { 2 });
            assert_eq!(bus.memory[0x1fd], 0x80);
            assert_eq!(bus.memory[0x1fb] & flags::B != 0, !hardware_irq);
            assert_eq!(bus.memory[0x1fb] & flags::I, 0);
            // Both early and late edges allow the first handler instruction.
            assert_eq!(cpu.step(&mut bus), 2);
            assert_eq!(cpu.x, 1);
            if edge > 4 {
                assert_eq!(cpu.step(&mut bus), 7);
                assert_eq!(cpu.pc, 0xa000);
            }
        }
    }
}
