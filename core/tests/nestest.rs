//! nestest: the standard 6502 conformance ROM, compared against its reference log.
//!
//! Skips with a printed note when the ROM is absent - see `roms/README.md`.
//!
//! The comparison is on *parsed register state*, not on log text. Matching text
//! would mean reproducing nestest.log's operand annotations exactly, which tests
//! the formatter rather than the CPU and reports every failure as "line differs".
//! Comparing fields points at the single register that went wrong.

use nes_core::Nes;
use std::path::Path;

/// One parsed line of the reference log.
#[derive(Debug, PartialEq, Eq)]
struct Expected {
    pc: u16,
    a: u8,
    x: u8,
    y: u8,
    p: u8,
    s: u8,
    cyc: u64,
}

fn parse_line(line: &str) -> Option<Expected> {
    let field = |key: &str| -> Option<&str> {
        let i = line.find(key)? + key.len();
        Some(line[i..].split_whitespace().next()?)
    };
    Some(Expected {
        pc: u16::from_str_radix(line.get(0..4)?, 16).ok()?,
        a: u8::from_str_radix(field("A:")?, 16).ok()?,
        x: u8::from_str_radix(field("X:")?, 16).ok()?,
        y: u8::from_str_radix(field("Y:")?, 16).ok()?,
        p: u8::from_str_radix(field("P:")?, 16).ok()?,
        s: u8::from_str_radix(field("SP:")?, 16).ok()?,
        cyc: field("CYC:")?.parse().ok()?,
    })
}

#[test]
fn nestest_matches_the_reference_log() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/roms");
    let (rom_path, log_path) = (dir.join("nestest.nes"), dir.join("nestest.log"));

    if !rom_path.exists() || !log_path.exists() {
        eprintln!(
            "skipping nestest: place nestest.nes and nestest.log in {} \
             (see roms/README.md)",
            dir.display()
        );
        return;
    }

    let rom = std::fs::read(&rom_path).expect("read nestest.nes");
    let log = std::fs::read_to_string(&log_path).expect("read nestest.log");
    let expected: Vec<Expected> = log.lines().filter_map(parse_line).collect();
    assert!(!expected.is_empty(), "nestest.log parsed to zero lines");

    let mut nes = Nes::new(&rom).expect("load nestest.nes");
    // Automation mode: no PPU or input needed, and the log starts at cycle 7.
    nes.cpu.set_pc(0xC000);
    nes.cpu.cycles = 7;

    let mut history: Vec<String> = Vec::new();
    for (i, want) in expected.iter().enumerate() {
        let got = nes.step_traced();
        history.push(got.format());

        let actual = Expected {
            pc: got.pc,
            a: got.a,
            x: got.x,
            y: got.y,
            p: got.p,
            s: got.s,
            cyc: got.cycles,
        };
        if &actual != want {
            let context = history.iter().rev().take(5).rev().cloned().collect::<Vec<_>>();
            panic!(
                "diverged at log line {}\n  expected {:x?}\n  actual   {:x?}\n\nlast instructions:\n  {}",
                i + 1,
                want,
                actual,
                context.join("\n  ")
            );
        }
    }

    // nestest reports failures as non-zero bytes at $0002 and $0003.
    let (r2, r3) = (nes.bus.read_pure(0x0002), nes.bus.read_pure(0x0003));
    assert_eq!((r2, r3), (0, 0), "nestest reported errors at $0002/$0003");
}
