mod common;

use nes_core::{cpu::Bus, Buttons, Cartridge, Nes, NesBus};

fn bus() -> NesBus {
    NesBus::new(Cartridge::load(&common::rom(0, 0)).unwrap())
}

#[test]
fn oam_dma_waits_for_a_read_after_the_writing_instruction() {
    let mut nes = Nes::new(&common::rom(0, 0)).unwrap();
    nes.cpu.pc = 0x300;
    nes.bus.ram[0x300..0x306].copy_from_slice(&[0xa9, 2, 0x8d, 0x14, 0x40, 0xea]);
    nes.bus.ram[0x200..0x300].fill(0x5a);
    assert_eq!(nes.step(), 2); // LDA #2.
    assert_eq!(nes.step(), 4); // STA $4014 queues DMA.
    assert_eq!(nes.bus.ppu.oam[0], 0);
    // Additional CPU writes cannot be halted and can still change the source.
    nes.bus.write(0x200, 0xa5);
    assert_eq!(nes.bus.ppu.oam[0], 0);
    let before = nes.cpu.cycles;
    let elapsed = nes.step(); // NOP's opcode read halts for the transfer.
    assert!((515..=516).contains(&elapsed));
    assert_eq!(nes.cpu.cycles - before, elapsed);
    assert_eq!(nes.bus.ppu.oam[0], 0xa5);
    assert_eq!(nes.bus.ppu.oam[1], 0x5a);
    nes.bus.ppu.oam_addr = 2;
    assert_eq!(nes.bus.ppu.read_register(0x2004, nes.bus.cart.mapper.as_mut()), 0x42);
    assert_eq!(nes.bus.ppu.oam[255], 0x5a);
    assert_eq!(nes.bus.dma_stall, 0);
    assert_eq!(nes.step(), 7, "the next BRK must not inherit DMA cycles");
}

#[test]
fn dmc_halt_and_dummy_cycles_repeat_ppudata_reads() {
    for phase in 0..2 {
        let mut bus = bus();
        bus.apu.write_register(0x4015, 0x10);
        for _ in 0..3 + phase { bus.write(0, 0); }
        assert!(bus.apu.dmc_request().is_some());
        bus.ppu.v = 0x2000;
        bus.read(0x2007);
        assert!(bus.read_halted());
        // Three-cycle DMA repeats halt/dummy/CPU reads; a four-cycle DMA has
        // an alignment read as well. The sample transfer uses its own address.
        assert_eq!(bus.ppu.v, 0x2003 + phase);
    }
}

#[test]
fn oam_dma_activates_apu_registers_only_when_cpu_holds_an_io_address() {
    for cpu_address in [0x4000, 0x8000] {
        let mut bus = bus();
        bus.write(0x4015, 4);
        bus.write(0x400b, 8); // Enable triangle length status.
        bus.ram[0x200..0x280].fill(0xff);
        bus.ram[0x280..0x300].fill(0);
        bus.write(0x4014, 2);
        bus.read(cpu_address);
        let active = cpu_address == 0x4000;
        assert_eq!(bus.ppu.oam[0x15], if active { 0x24 } else { 0xff });
        assert_eq!(bus.ppu.oam[0x95], if active { 4 } else { 0 });
    }
}

#[test]
fn contiguous_controller_reads_clock_only_once() {
    let mut bus = bus();
    bus.controllers[0].set_buttons(Buttons(Buttons::A));
    bus.controllers[0].write_strobe(1);
    bus.controllers[0].write_strobe(0);
    for _ in 0..4 { assert_eq!(bus.read(0x4016) & 1, 1); }
    bus.read(0); // Deassert /OE before the next button read.
    assert_eq!(bus.read(0x4016) & 1, 0);
}

#[test]
fn implicit_dma_abort_halts_once_or_expires_during_a_write() {
    for write_during_abort in [false, true] {
        let mut bus = bus();
        bus.apu.write_register(0x4010, 0x0e); // 72-cycle output timer, no looping.
        // The eighth output clock is cycle 506. Arrange the one-byte load's
        // transfer on cycle 504, immediately before that output reload.
        for _ in 0..497 { bus.read(0); }
        bus.write(0x4015, 0x10); // Cycle 498; load request appears after 501.
        for _ in 0..3 { bus.read(0); assert!(!bus.read_halted()); }
        bus.read(0); // Halt 502, dummy 503, sample 504, CPU read 505.
        assert!(bus.read_halted());
        bus.read(0); // Output reload at 506 briefly requests another transfer.
        assert!(!bus.read_halted());
        assert!(bus.apu.dmc_request().is_some());
        let before = bus.ppu.scanline as u32 * 341 + bus.ppu.dot as u32;
        if write_during_abort { bus.write(0, 0); } else { bus.read(0); }
        let after = bus.ppu.scanline as u32 * 341 + bus.ppu.dot as u32;
        assert_eq!(after - before, if write_during_abort { 3 } else { 6 });
        assert_eq!(bus.read_halted(), !write_during_abort);
        assert!(bus.apu.dmc_request().is_none());
        bus.read(0);
        assert!(!bus.read_halted(), "a cancelled halt must not resume later");
    }
}

#[test]
fn reset_accounts_for_dma_without_charging_the_next_instruction() {
    let mut nes = Nes::new(&common::rom(0, 0)).unwrap();
    nes.bus.ram[0x200..0x300].fill(0xa5);
    nes.bus.write(0x4014, 2);
    let before = nes.cpu.cycles;
    nes.reset();
    assert!((520..=521).contains(&(nes.cpu.cycles - before)));
    assert_eq!(nes.bus.ppu.oam[0], 0xa5);
    // Run a known NOP independently of the synthetic cartridge reset vector.
    nes.cpu.pc = 0x300;
    nes.bus.ram[0x300] = 0xea;
    assert_eq!(nes.step(), 2);
}

#[test]
fn explicit_dma_abort_only_releases_rdy_early_during_the_halt_cycle() {
    for (disable_cycle, stolen) in [(503, 0), (504, 1), (505, 1), (506, 4), (507, 3)] {
        let mut bus = bus();
        bus.apu.write_register(0x4010, 0x4e); // Loop at 72 cycles per output bit.
        bus.write(0x4015, 0x10);
        let cycle = |bus: &NesBus| (bus.ppu.scanline as u32 * 341 + bus.ppu.dot as u32) / 3;
        while cycle(&bus) < disable_cycle - 1 { bus.read(0); }
        bus.write(0x4015, 0);
        assert_eq!(cycle(&bus), disable_cycle);
        let mut elapsed = 0;
        // Reads leading up to the reload are unaffected. Sum the extra cycles
        // around it so both write phases and all cancellation windows are tested.
        for _ in 0..6 {
            let before = cycle(&bus);
            bus.read(0);
            elapsed += cycle(&bus) - before - 1;
        }
        assert_eq!(elapsed, stolen, "disable at cycle {disable_cycle}");
        assert!(bus.apu.dmc_request().is_none());
    }
}

#[test]
fn controller_bus_conflicts_do_not_replace_zero_bits_in_dmc_audio() {
    let mut rom = common::rom(0, 0);
    rom[16 + 0x4000..16 + 0x4040].fill(0); // Includes $C016's contested bit.
    let mut conflicted = NesBus::new(Cartridge::load(&rom).unwrap());
    conflicted.controllers[0].set_buttons(Buttons(Buttons::A));
    conflicted.controllers[0].write_strobe(1);
    conflicted.controllers[0].write_strobe(0);
    conflicted.write(0x4010, 0x0f);
    conflicted.write(0x4011, 0x40);
    conflicted.write(0x4013, 2); // 33 bytes cross both controller port addresses.
    conflicted.write(0x4015, 0x10);
    let mut ordinary = conflicted.clone();
    ordinary.controllers[0].set_buttons(Buttons(0));
    ordinary.controllers[0].write_strobe(1);
    ordinary.controllers[0].write_strobe(0);
    let mut saw_controller_bit = false;
    for _ in 0..16000 {
        let value = conflicted.read(0x4000);
        if conflicted.read_halted() && value & 1 != 0 { saw_controller_bit = true; }
        ordinary.read(0x4000);
    }
    assert!(saw_controller_bit, "the CPU must observe the internal controller bit");
    assert!(conflicted.apu.samples() == ordinary.apu.samples(),
        "DMC audio must sample the external zero even while the CPU sees one");
}
