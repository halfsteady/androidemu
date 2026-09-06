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
    assert!(!a.irq_line());
    a.write_register(0x4017, 0x40);
    for _ in 0..60000 {
        a.tick();
    }
    assert!(!a.irq_line());
}
