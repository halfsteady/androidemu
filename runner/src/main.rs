//! Headless runner: the CI harness and the debugging front end.
//!
//! ```text
//!   nes-runner info   <rom>              header summary
//!   nes-runner trace  <rom> [n] [--pc=X] execution trace in nestest.log layout
//!   nes-runner frames <rom> <n>          run N frames, print a framebuffer hash
//! ```

use nes_core::Nes;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: nes-runner <info|trace|frames> <rom> [args]");
        return ExitCode::from(2);
    }

    let rom = match std::fs::read(&args[1]) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("cannot read {}: {e}", args[1]);
            return ExitCode::from(1);
        }
    };

    let mut nes = match Nes::new(&rom) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("cannot load {}: {e}", args[1]);
            return ExitCode::from(1);
        }
    };

    match args[0].as_str() {
        "info" => {
            let h = &nes.bus.cart.header;
            println!("mapper       {} (submapper {})", h.mapper, h.submapper);
            println!("format       {}", if h.nes2 { "NES 2.0" } else { "iNES" });
            println!("prg rom      {} KB", h.prg_rom_size / 1024);
            println!(
                "chr          {}",
                if h.chr_rom_size == 0 {
                    format!("{} KB RAM", h.chr_ram_size / 1024)
                } else {
                    format!("{} KB ROM", h.chr_rom_size / 1024)
                }
            );
            println!("prg ram      {} KB", h.prg_ram_size / 1024);
            println!("mirroring    {:?}", h.mirroring);
            println!("battery      {}", h.battery);
            println!("region       {:?}", h.region);
            println!("hash         {:016x}", h.hash);
        }
        "trace" => {
            let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100);
            for a in &args[2..] {
                if let Some(hex) = a.strip_prefix("--pc=") {
                    let pc = u16::from_str_radix(hex.trim_start_matches("0x"), 16)
                        .expect("--pc expects hex");
                    nes.cpu.set_pc(pc);
                    // nestest's automation mode expects the CPU to have already
                    // burned its 7 reset cycles.
                    nes.cpu.cycles = 7;
                }
            }
            for _ in 0..n {
                println!("{}", nes.step_traced().format());
            }
        }
        "frames" => {
            let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(60);
            for _ in 0..n {
                nes.step_frame();
            }
            let fb = nes.framebuffer();
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for &b in fb {
                h ^= b as u64;
                h = h.wrapping_mul(0x100_0000_01b3);
            }
            println!("frames {n}  cycles {}  hash {:016x}", nes.cpu.cycles, h);
        }
        other => {
            eprintln!("unknown command: {other}");
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}
