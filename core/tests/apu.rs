use nes_core::apu::Apu;
#[test]
fn pulse_produces_finite_audio_and_length_status() {
    let mut a = Apu::new();
    a.write_register(0x4015, 1);
    a.write_register(0x4000, 0x9f);
    a.write_register(0x4002, 100);
    a.write_register(0x4003, 0x08);
    assert_eq!(a.read_register(0x4015) & 1, 1);
    for _ in 0..29830 {
        a.tick();
    }
    assert!((799..=801).contains(&a.samples().len()));
    assert!(a.samples().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    assert!(a.samples().windows(2).any(|s| (s[1] - s[0]).abs() > 0.001));
    a.write_register(0x4015, 0);
    assert_eq!(a.read_register(0x4015) & 15, 0);
}
#[test]
fn dmc_fetches_wrap_loop_and_irq() {
    let mut a = Apu::new();
    a.write_register(0x4010, 0x8f);
    a.write_register(0x4012, 0xff);
    a.write_register(0x4013, 4);
    a.write_register(0x4015, 0x10);
    let mut addresses = Vec::new();
    for _ in 0..40000 {
        if let Some(addr) = a.dmc_request() {
            addresses.push(addr);
            a.supply_dmc(0xff);
        }
        a.tick();
    }
    assert_eq!(addresses.len(), 65);
    assert_eq!(addresses[0], 0xffc0);
    assert_eq!(addresses[64], 0x8000);
    assert_ne!(a.read_register(0x4015) & 0x80, 0);
    a.tick(); // Let the simultaneous frame IRQ acknowledgement settle.
    a.tick();
    assert!(a.irq_line());
    a.write_register(0x4015, 0);
    assert!(!a.irq_line());
}
#[test]
fn frame_irq_acknowledge_and_inhibit() {
    let mut a = Apu::new();
    for _ in 0..29830 {
        a.tick();
    }
    assert!(a.irq_line());
    assert_eq!(a.read_register(0x4015) & 0x40, 0x40);
    a.tick(); // Acknowledgement takes effect on the next get phase.
    a.tick();
    assert!(!a.irq_line());
    a.write_register(0x4017, 0x40);
    for _ in 0..60000 {
        a.tick();
    }
    assert!(!a.irq_line());
}

#[test]
fn status_acknowledgement_waits_for_the_next_get_cycle() {
    for phase in 0..2 {
        let mut apu = Apu::new();
        for _ in 0..30000 + phase { apu.tick(); }
        assert_eq!(apu.read_register(0x4015) & 0x40, 0x40);
        assert!(apu.irq_line(), "the read must not clear the flag immediately");
        apu.tick();
        assert_eq!(apu.read_register(0x4015) & 0x40, if phase == 0 { 0x40 } else { 0 });
        apu.tick();
        assert!(!apu.irq_line());
    }
}

#[test]
fn frame_counter_reset_delay_depends_on_the_write_phase_in_both_modes() {
    for phase in 0..2 {
        for five_step in [false, true] {
            let mut apu = Apu::new();
            for _ in 0..phase { apu.tick(); }
            apu.write_register(0x4015, 1);
            apu.write_register(0x4003, 0x18); // Two half-frame clocks of length.
            apu.write_register(0x4017, if five_step { 0x80 } else { 0 });
            let delay = if phase == 0 { 4 } else { 3 };
            // Five-step reset immediately clocks the first half-frame; four-step
            // mode waits for the normal half-frame and end-of-frame clocks.
            let expires = delay + if five_step { 14913 } else { 29829 };
            for _ in 0..expires - 1 { apu.tick(); }
            assert_eq!(apu.read_register(0x4015) & 1, 1);
            apu.tick();
            assert_eq!(apu.read_register(0x4015) & 1, 0);
        }
    }
}
