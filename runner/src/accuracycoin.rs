//! AccuracyCoin's menu/result protocol, separate from blargg's $6000 protocol.
use nes_core::{Buttons, Nes};

pub fn run(nes: &mut Nes, limit: usize) -> Result<bool, String> {
    // Current upstream's 22 page pointers live at $8100. Read names and result
    // addresses from the ROM itself, excluding the five DRAW entries in page 3.
    let word = |nes: &mut Nes, addr| {
        u16::from_le_bytes([nes.bus.read_pure(addr), nes.bus.read_pure(addr + 1)])
    };
    let mut tests = Vec::new();
    for page in 0..22 {
        let mut ptr = word(nes, 0x8100 + page * 2);
        if ptr < 0x812c {
            return Err("Unrecognized AccuracyCoin menu layout".into());
        }
        let mut string = |nes: &mut Nes| -> Result<String, String> {
            let mut bytes = Vec::new();
            for _ in 0..80 {
                let b = nes.bus.read_pure(ptr);
                ptr = ptr.checked_add(1).ok_or("Invalid menu pointer")?;
                if b == 0xff {
                    return String::from_utf8(bytes).map_err(|e| e.to_string());
                }
                bytes.push(b);
            }
            Err("Invalid AccuracyCoin menu string".into())
        };
        let _title = string(nes)?;
        for _ in 0..16 {
            if nes.bus.read_pure(ptr) == 0xff {
                break;
            }
            let mut bytes = Vec::new();
            while nes.bus.read_pure(ptr) != 0xff {
                bytes.push(nes.bus.read_pure(ptr));
                ptr = ptr.checked_add(1).ok_or("Invalid test pointer")?;
                if bytes.len() > 80 {
                    return Err("Invalid test name".into());
                }
            }
            ptr = ptr.checked_add(1).ok_or("Invalid result pointer")?;
            let next = ptr.checked_add(4).ok_or("Truncated test entry")?;
            let result = word(nes, ptr);
            ptr = next;
            if (0x400..0x500).contains(&result) {
                tests.push((
                    page + 1,
                    String::from_utf8_lossy(&bytes).into_owned(),
                    result,
                ));
            } else if !(0x300..0x400).contains(&result) {
                return Err("Invalid AccuracyCoin result address".into());
            }
        }
    }
    if tests.len() != 144 {
        return Err(format!(
            "Expected 144 AccuracyCoin tests, found {}",
            tests.len()
        ));
    }
    let mut started = false;
    let mut pressed = None;
    let mut completed = false;
    for frame in 0..limit {
        if pressed.is_none() && nes.bus.ram[0xec] == 10 {
            nes.set_buttons(0, Buttons(Buttons::START));
            pressed = Some(frame);
        }
        if pressed.is_some_and(|f| frame >= f + 2) {
            nes.set_buttons(0, Buttons(0));
        }
        nes.step_frame();
        if frame % 1000 == 999 {
            eprintln!("AccuracyCoin: {} frames elapsed", frame + 1);
        }
        if nes.bus.ram[0x35] != 0 {
            started = true;
        }
        if started && nes.bus.ram[0x35] == 0 && nes.bus.ram[0x37] == 144 && nes.bus.ram[0x495] != 0
        {
            completed = true;
            break;
        }
    }
    let mut passed = 0;
    for (page, name, addr) in tests {
        let value = nes.bus.ram[addr as usize];
        let status = match value {
            0 => "PENDING",
            3 => "RUNNING",
            0xff => "SKIP",
            v if v & 1 != 0 => {
                passed += 1;
                "PASS"
            }
            _ => "FAIL",
        };
        println!("{status}\t{page}\t{name}\t{:02X}\t${addr:04X}", value >> 2);
    }
    println!("AccuracyCoin: {passed}/144 passed; completed={completed}");
    if !completed {
        return Err(format!(
            "AccuracyCoin did not finish within {limit} frames (PC=${:04X})",
            nes.cpu.pc
        ));
    }
    Ok(passed == 144)
}
