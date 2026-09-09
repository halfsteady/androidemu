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
        self.irq
    }
}
fn interrupt_machine(edge: usize) -> (Cpu, EdgeBus) {
    let mut bus = EdgeBus {
        memory: vec![0; 65536],
        cycle: 0,
        edge,
        irq: false,
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
