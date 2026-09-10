//! AccuracyCoin's menu/result protocol, separate from blargg's $6000 protocol.
use nes_core::{Buttons, Nes};

const TEST_COUNT: usize = 144;

#[derive(Debug)]
struct Test {
    page: u16,
    name: String,
    address: u16,
}

fn word(nes: &mut Nes, addr: u16) -> Result<u16, String> {
    let high = addr.checked_add(1).ok_or("Truncated menu word")?;
    Ok(u16::from_le_bytes([
        nes.bus.read_pure(addr),
        nes.bus.read_pure(high),
    ]))
}

fn menu_string(nes: &mut Nes, ptr: &mut u16) -> Result<String, String> {
    let mut name = String::new();
    for _ in 0..=80 {
        let b = nes.bus.read_pure(*ptr);
        *ptr = ptr.checked_add(1).ok_or("Invalid menu pointer")?;
        if b == 0xff && !name.is_empty() {
            return Ok(name);
        }
        // Names are printable ASCII. In particular, reject tabs/newlines that
        // would corrupt the machine-readable result rows.
        if !(0x20..=0x7e).contains(&b) {
            return Err("Invalid AccuracyCoin menu string".into());
        }
        name.push(char::from(b));
    }
    Err("AccuracyCoin menu string is too long".into())
}

fn menu(nes: &mut Nes) -> Result<Vec<Test>, String> {
    // The pinned revision's 22 page pointers live at $8100. Read names and
    // result addresses from the ROM, excluding the five DRAW entries in RAM
    // page $03. Those share a scratch address instead of storing test results.
    let mut tests = Vec::new();
    let mut pages = std::collections::BTreeSet::new();
    let mut addresses = std::collections::BTreeSet::new();
    let mut drawings = 0;
    for page in 1..=22 {
        let mut ptr = word(nes, 0x8100 + (page - 1) * 2)?;
        if ptr < 0x812c || !pages.insert(ptr) {
            return Err("Unrecognized AccuracyCoin menu layout".into());
        }
        menu_string(nes, &mut ptr)?;
        for _ in 0..16 {
            if nes.bus.read_pure(ptr) == 0xff {
                break;
            }
            let name = menu_string(nes, &mut ptr)?;
            let next = ptr.checked_add(4).ok_or("Truncated test entry")?;
            let address = word(nes, ptr)?;
            ptr = next;
            if (0x400..0x500).contains(&address) {
                if !addresses.insert(address) {
                    return Err("Duplicate AccuracyCoin result address".into());
                }
                tests.push(Test {
                    page,
                    name,
                    address,
                });
            } else if (0x300..0x400).contains(&address) {
                drawings += 1;
            } else {
                return Err("Invalid AccuracyCoin result address".into());
            }
        }
        if nes.bus.read_pure(ptr) != 0xff {
            return Err("Unterminated AccuracyCoin menu page".into());
        }
    }
    if tests.len() != TEST_COUNT || drawings != 5 {
        return Err(format!(
            "Expected {TEST_COUNT} AccuracyCoin tests and 5 drawings, found {} and {drawings}",
            tests.len()
        ));
    }
    Ok(tests)
}

fn status(value: u8) -> &'static str {
    match value {
        0 => "PENDING",
        3 => "RUNNING",
        0xff => "SKIP",
        v if v & 3 == 1 => "PASS",
        v if v & 3 == 2 => "FAIL",
        _ => "INVALID",
    }
}

pub fn run(nes: &mut Nes, limit: usize) -> Result<bool, String> {
    let tests = menu(nes)?;
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
    let mut finished = true;
    for Test {
        page,
        name,
        address: addr,
    } in tests
    {
        let value = nes.bus.ram[addr as usize];
        let status = status(value);
        passed += usize::from(status == "PASS");
        finished &= matches!(status, "PASS" | "FAIL");
        println!("{status}\t{page}\t{name}\t{:02X}\t${addr:04X}", value >> 2);
    }
    let ended = completed;
    completed &= finished;
    println!("AccuracyCoin: {passed}/{TEST_COUNT} passed; completed={completed}");
    if ended && !finished {
        return Err("AccuracyCoin ended with skipped, unfinished or invalid results".into());
    }
    if !completed {
        return Err(format!(
            "AccuracyCoin did not finish within {limit} frames (PC=${:04X})",
            nes.cpu.pc
        ));
    }
    Ok(passed == TEST_COUNT)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Only menu metadata is needed to test layout validation; no upstream ROM
    // or emulated test outcomes are bundled here.
    fn fixture() -> (Vec<u8>, Vec<usize>, Vec<usize>) {
        let mut rom = vec![0; 16 + 32768];
        rom[..4].copy_from_slice(b"NES\x1a");
        rom[4] = 2;
        let mut ptr = 16 + 0x12c;
        let mut entries = Vec::new();
        let mut pages = Vec::new();
        let mut result = 0x400u16;
        for page in 0..22 {
            pages.push(ptr);
            let addr = (ptr - 16) as u16 + 0x8000;
            rom[16 + 0x100 + page * 2..16 + 0x102 + page * 2].copy_from_slice(&addr.to_le_bytes());
            rom[ptr..ptr + 2].copy_from_slice(b"P\xff");
            ptr += 2;
            for _ in 0..if page < 12 { 7 } else { 6 } {
                rom[ptr..ptr + 2].copy_from_slice(b"T\xff");
                entries.push(ptr + 2);
                rom[ptr + 2..ptr + 4].copy_from_slice(&result.to_le_bytes());
                result += 1;
                ptr += 6;
            }
            if page == 15 {
                for _ in 0..5 {
                    rom[ptr..ptr + 2].copy_from_slice(b"D\xff");
                    rom[ptr + 2..ptr + 4].copy_from_slice(&0x3ffu16.to_le_bytes());
                    ptr += 6;
                }
            }
            rom[ptr] = 0xff;
            ptr += 1;
        }
        (rom, entries, pages)
    }

    #[test]
    fn parses_all_tests_and_excludes_drawings() {
        let (rom, _, _) = fixture();
        let tests = menu(&mut Nes::new(&rom).unwrap()).unwrap();
        assert_eq!(tests.len(), TEST_COUNT);
        assert_eq!(tests.first().unwrap().address, 0x400);
        assert_eq!(tests.last().unwrap().address, 0x48f);
        assert_eq!(tests.last().unwrap().page, 22);
    }

    #[test]
    fn rejects_duplicate_results_even_when_test_count_matches() {
        let (mut rom, entries, _) = fixture();
        let first = [rom[entries[0]], rom[entries[0] + 1]];
        rom[entries[1]..entries[1] + 2].copy_from_slice(&first);
        assert!(menu(&mut Nes::new(&rom).unwrap())
            .unwrap_err()
            .contains("Duplicate"));
    }

    #[test]
    fn rejects_aliased_pages_and_bad_strings_or_pointers() {
        let (rom, _, pages) = fixture();
        for (offset, replacement) in [
            (16 + 0x102, rom[16 + 0x100..16 + 0x102].to_vec()),
            (16 + 0x100, vec![0xff, 0xff]),
            (pages[0], vec![b'\t']),
            (pages[0] + 2, vec![0x80]),
        ] {
            let mut broken = rom.clone();
            broken[offset..offset + replacement.len()].copy_from_slice(&replacement);
            assert!(menu(&mut Nes::new(&broken).unwrap()).is_err());
        }
    }

    #[test]
    fn result_tags_distinguish_pass_variants_from_unfinished_data() {
        assert_eq!(status(0), "PENDING");
        assert_eq!(status(3), "RUNNING");
        assert_eq!(status(0xff), "SKIP");
        for value in [1, 5, 9, 0xfd] {
            assert_eq!(status(value), "PASS");
        }
        for value in [2, 6, 0xfe] {
            assert_eq!(status(value), "FAIL");
        }
        for value in [4, 7, 0xfc] {
            assert_eq!(status(value), "INVALID");
        }
    }

    #[test]
    fn completion_counters_cannot_hide_unfinished_results() {
        let (mut rom, _, _) = fixture();
        // Synthetic protocol driver: mark the run active, wait two vblanks,
        // publish completion counters, then loop. Individual results are test
        // inputs in RAM, so this exercises the actual frame/completion path.
        let program = [
            0xa9, 1, 0x85, 0x35, 0x2c, 2, 0x20, 0x10, 0xfb, 0x2c, 2, 0x20, 0x10, 0xfb, 0xa9, 0,
            0x85, 0x35, 0xa9, 144, 0x85, 0x37, 0x4c, 0x16, 0x80,
        ];
        rom[16..16 + program.len()].copy_from_slice(&program);
        rom[16 + 0x7ffc..16 + 0x7ffe].copy_from_slice(&[0, 0x80]);
        for result in [1, 0, 3, 0xff, 4, 7] {
            let mut nes = Nes::new(&rom).unwrap();
            nes.bus.ram[0x400..0x490].fill(1);
            nes.bus.ram[0x400] = result;
            nes.bus.ram[0x495] = 1;
            let outcome = run(&mut nes, 3);
            if result == 1 {
                assert_eq!(outcome, Ok(true));
            } else {
                assert!(outcome.unwrap_err().contains("unfinished or invalid"));
            }
        }
    }
}
