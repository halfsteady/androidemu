//! Disassembly, for traces and debugging.
//!
//! Note: this deliberately does *not* reproduce nestest.log's operand annotations
//! (`$0200,X @ 0205 = A9`). Those require side-effect-free peeks into the whole
//! address space, and the test harness compares parsed register state rather than
//! log text, so the annotations would be cost without benefit.

use super::table::{Mode, OPS};

/// Disassemble one instruction. `bytes` must hold at least the instruction's own
/// length; `pc` is the address of the opcode.
pub fn disasm(pc: u16, bytes: &[u8]) -> String {
    let entry = OPS[bytes[0] as usize];
    let mnemonic = entry.op.mnemonic();
    let marker = if entry.official { ' ' } else { '*' };

    let b = |i: usize| -> u8 { bytes.get(i).copied().unwrap_or(0) };
    let word = || -> u16 { b(1) as u16 | ((b(2) as u16) << 8) };

    let operand = match entry.mode {
        Mode::Imp => String::new(),
        Mode::Acc => "A".to_string(),
        Mode::Imm => format!("#${:02X}", b(1)),
        Mode::Zp => format!("${:02X}", b(1)),
        Mode::ZpX => format!("${:02X},X", b(1)),
        Mode::ZpY => format!("${:02X},Y", b(1)),
        Mode::Abs => format!("${:04X}", word()),
        Mode::AbsX => format!("${:04X},X", word()),
        Mode::AbsY => format!("${:04X},Y", word()),
        Mode::Ind => format!("(${:04X})", word()),
        Mode::IndX => format!("(${:02X},X)", b(1)),
        Mode::IndY => format!("(${:02X}),Y", b(1)),
        Mode::Rel => {
            let target = (pc as i32 + 2 + b(1) as i8 as i32) as u16;
            format!("${:04X}", target)
        }
    };

    if operand.is_empty() {
        format!("{marker}{mnemonic}")
    } else {
        format!("{marker}{mnemonic} {operand}")
    }
}

/// Instruction length in bytes, for stepping a disassembly listing forward.
pub fn length(opcode: u8) -> u16 {
    OPS[opcode as usize].mode.len()
}
