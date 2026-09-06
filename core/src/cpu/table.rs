//! The 6502 opcode table: one entry per opcode byte, giving the operation, the
//! addressing mode, and how the operand is accessed.
//!
//! `Access` is not cosmetic - it decides whether an indexed addressing mode performs
//! its extra dummy read. A read that stays inside a page skips it; a write always
//! performs it. Getting this wrong desynchronises the PPU by a cycle on a large
//! fraction of instructions.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Imp,
    Acc,
    Imm,
    Zp,
    ZpX,
    ZpY,
    Abs,
    AbsX,
    AbsY,
    Ind,
    IndX,
    IndY,
    Rel,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Access {
    /// No operand fetch beyond the address itself (JMP, JSR, branches, implied).
    None,
    Read,
    Write,
    /// Read, dummy-write the old value, write the new one.
    Rmw,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[rustfmt::skip]
pub enum Op {
    // Official
    Adc, And, Asl, Bcc, Bcs, Beq, Bit, Bmi, Bne, Bpl, Brk, Bvc, Bvs, Clc,
    Cld, Cli, Clv, Cmp, Cpx, Cpy, Dec, Dex, Dey, Eor, Inc, Inx, Iny, Jmp,
    Jsr, Lda, Ldx, Ldy, Lsr, Nop, Ora, Pha, Php, Pla, Plp, Rol, Ror, Rti,
    Rts, Sbc, Sec, Sed, Sei, Sta, Stx, Sty, Tax, Tay, Tsx, Txa, Txs, Tya,
    // Unofficial - real games do use these, and nestest exercises all of them.
    Lax, Sax, Dcp, Isb, Slo, Rla, Sre, Rra, Anc, Alr, Arr, Axs, Xaa,
    Ahx, Shy, Shx, Tas, Las, Kil,
}

#[derive(Clone, Copy)]
pub struct Entry {
    pub op: Op,
    pub mode: Mode,
    pub access: Access,
    /// False for the undocumented opcodes; the disassembler marks these with `*`,
    /// matching the convention nestest.log uses.
    pub official: bool,
}

const fn e(op: Op, mode: Mode, access: Access) -> Entry {
    Entry { op, mode, access, official: true }
}
const fn u(op: Op, mode: Mode, access: Access) -> Entry {
    Entry { op, mode, access, official: false }
}

use Access::*;
use Mode::*;
use Op::*;

#[rustfmt::skip]
pub static OPS: [Entry; 256] = [
    /* 00 */ e(Brk,Imp,None), e(Ora,IndX,Read), u(Kil,Imp,None), u(Slo,IndX,Rmw),
    /* 04 */ u(Nop,Zp,Read),  e(Ora,Zp,Read),   e(Asl,Zp,Rmw),   u(Slo,Zp,Rmw),
    /* 08 */ e(Php,Imp,None), e(Ora,Imm,Read),  e(Asl,Acc,None), u(Anc,Imm,Read),
    /* 0C */ u(Nop,Abs,Read), e(Ora,Abs,Read),  e(Asl,Abs,Rmw),  u(Slo,Abs,Rmw),
    /* 10 */ e(Bpl,Rel,None), e(Ora,IndY,Read), u(Kil,Imp,None), u(Slo,IndY,Rmw),
    /* 14 */ u(Nop,ZpX,Read), e(Ora,ZpX,Read),  e(Asl,ZpX,Rmw),  u(Slo,ZpX,Rmw),
    /* 18 */ e(Clc,Imp,None), e(Ora,AbsY,Read), u(Nop,Imp,None), u(Slo,AbsY,Rmw),
    /* 1C */ u(Nop,AbsX,Read),e(Ora,AbsX,Read), e(Asl,AbsX,Rmw), u(Slo,AbsX,Rmw),
    /* 20 */ e(Jsr,Abs,None), e(And,IndX,Read), u(Kil,Imp,None), u(Rla,IndX,Rmw),
    /* 24 */ e(Bit,Zp,Read),  e(And,Zp,Read),   e(Rol,Zp,Rmw),   u(Rla,Zp,Rmw),
    /* 28 */ e(Plp,Imp,None), e(And,Imm,Read),  e(Rol,Acc,None), u(Anc,Imm,Read),
    /* 2C */ e(Bit,Abs,Read), e(And,Abs,Read),  e(Rol,Abs,Rmw),  u(Rla,Abs,Rmw),
    /* 30 */ e(Bmi,Rel,None), e(And,IndY,Read), u(Kil,Imp,None), u(Rla,IndY,Rmw),
    /* 34 */ u(Nop,ZpX,Read), e(And,ZpX,Read),  e(Rol,ZpX,Rmw),  u(Rla,ZpX,Rmw),
    /* 38 */ e(Sec,Imp,None), e(And,AbsY,Read), u(Nop,Imp,None), u(Rla,AbsY,Rmw),
    /* 3C */ u(Nop,AbsX,Read),e(And,AbsX,Read), e(Rol,AbsX,Rmw), u(Rla,AbsX,Rmw),
    /* 40 */ e(Rti,Imp,None), e(Eor,IndX,Read), u(Kil,Imp,None), u(Sre,IndX,Rmw),
    /* 44 */ u(Nop,Zp,Read),  e(Eor,Zp,Read),   e(Lsr,Zp,Rmw),   u(Sre,Zp,Rmw),
    /* 48 */ e(Pha,Imp,None), e(Eor,Imm,Read),  e(Lsr,Acc,None), u(Alr,Imm,Read),
    /* 4C */ e(Jmp,Abs,None), e(Eor,Abs,Read),  e(Lsr,Abs,Rmw),  u(Sre,Abs,Rmw),
    /* 50 */ e(Bvc,Rel,None), e(Eor,IndY,Read), u(Kil,Imp,None), u(Sre,IndY,Rmw),
    /* 54 */ u(Nop,ZpX,Read), e(Eor,ZpX,Read),  e(Lsr,ZpX,Rmw),  u(Sre,ZpX,Rmw),
    /* 58 */ e(Cli,Imp,None), e(Eor,AbsY,Read), u(Nop,Imp,None), u(Sre,AbsY,Rmw),
    /* 5C */ u(Nop,AbsX,Read),e(Eor,AbsX,Read), e(Lsr,AbsX,Rmw), u(Sre,AbsX,Rmw),
    /* 60 */ e(Rts,Imp,None), e(Adc,IndX,Read), u(Kil,Imp,None), u(Rra,IndX,Rmw),
    /* 64 */ u(Nop,Zp,Read),  e(Adc,Zp,Read),   e(Ror,Zp,Rmw),   u(Rra,Zp,Rmw),
    /* 68 */ e(Pla,Imp,None), e(Adc,Imm,Read),  e(Ror,Acc,None), u(Arr,Imm,Read),
    /* 6C */ e(Jmp,Ind,None), e(Adc,Abs,Read),  e(Ror,Abs,Rmw),  u(Rra,Abs,Rmw),
    /* 70 */ e(Bvs,Rel,None), e(Adc,IndY,Read), u(Kil,Imp,None), u(Rra,IndY,Rmw),
    /* 74 */ u(Nop,ZpX,Read), e(Adc,ZpX,Read),  e(Ror,ZpX,Rmw),  u(Rra,ZpX,Rmw),
    /* 78 */ e(Sei,Imp,None), e(Adc,AbsY,Read), u(Nop,Imp,None), u(Rra,AbsY,Rmw),
    /* 7C */ u(Nop,AbsX,Read),e(Adc,AbsX,Read), e(Ror,AbsX,Rmw), u(Rra,AbsX,Rmw),
    /* 80 */ u(Nop,Imm,Read), e(Sta,IndX,Write),u(Nop,Imm,Read), u(Sax,IndX,Write),
    /* 84 */ e(Sty,Zp,Write), e(Sta,Zp,Write),  e(Stx,Zp,Write), u(Sax,Zp,Write),
    /* 88 */ e(Dey,Imp,None), u(Nop,Imm,Read),  e(Txa,Imp,None), u(Xaa,Imm,Read),
    /* 8C */ e(Sty,Abs,Write),e(Sta,Abs,Write), e(Stx,Abs,Write),u(Sax,Abs,Write),
    /* 90 */ e(Bcc,Rel,None), e(Sta,IndY,Write),u(Kil,Imp,None), u(Ahx,IndY,Write),
    /* 94 */ e(Sty,ZpX,Write),e(Sta,ZpX,Write), e(Stx,ZpY,Write),u(Sax,ZpY,Write),
    /* 98 */ e(Tya,Imp,None), e(Sta,AbsY,Write),e(Txs,Imp,None), u(Tas,AbsY,Write),
    /* 9C */ u(Shy,AbsX,Write),e(Sta,AbsX,Write),u(Shx,AbsY,Write),u(Ahx,AbsY,Write),
    /* A0 */ e(Ldy,Imm,Read), e(Lda,IndX,Read), e(Ldx,Imm,Read), u(Lax,IndX,Read),
    /* A4 */ e(Ldy,Zp,Read),  e(Lda,Zp,Read),   e(Ldx,Zp,Read),  u(Lax,Zp,Read),
    /* A8 */ e(Tay,Imp,None), e(Lda,Imm,Read),  e(Tax,Imp,None), u(Lax,Imm,Read),
    /* AC */ e(Ldy,Abs,Read), e(Lda,Abs,Read),  e(Ldx,Abs,Read), u(Lax,Abs,Read),
    /* B0 */ e(Bcs,Rel,None), e(Lda,IndY,Read), u(Kil,Imp,None), u(Lax,IndY,Read),
    /* B4 */ e(Ldy,ZpX,Read), e(Lda,ZpX,Read),  e(Ldx,ZpY,Read), u(Lax,ZpY,Read),
    /* B8 */ e(Clv,Imp,None), e(Lda,AbsY,Read), e(Tsx,Imp,None), u(Las,AbsY,Read),
    /* BC */ e(Ldy,AbsX,Read),e(Lda,AbsX,Read), e(Ldx,AbsY,Read),u(Lax,AbsY,Read),
    /* C0 */ e(Cpy,Imm,Read), e(Cmp,IndX,Read), u(Nop,Imm,Read), u(Dcp,IndX,Rmw),
    /* C4 */ e(Cpy,Zp,Read),  e(Cmp,Zp,Read),   e(Dec,Zp,Rmw),   u(Dcp,Zp,Rmw),
    /* C8 */ e(Iny,Imp,None), e(Cmp,Imm,Read),  e(Dex,Imp,None), u(Axs,Imm,Read),
    /* CC */ e(Cpy,Abs,Read), e(Cmp,Abs,Read),  e(Dec,Abs,Rmw),  u(Dcp,Abs,Rmw),
    /* D0 */ e(Bne,Rel,None), e(Cmp,IndY,Read), u(Kil,Imp,None), u(Dcp,IndY,Rmw),
    /* D4 */ u(Nop,ZpX,Read), e(Cmp,ZpX,Read),  e(Dec,ZpX,Rmw),  u(Dcp,ZpX,Rmw),
    /* D8 */ e(Cld,Imp,None), e(Cmp,AbsY,Read), u(Nop,Imp,None), u(Dcp,AbsY,Rmw),
    /* DC */ u(Nop,AbsX,Read),e(Cmp,AbsX,Read), e(Dec,AbsX,Rmw), u(Dcp,AbsX,Rmw),
    /* E0 */ e(Cpx,Imm,Read), e(Sbc,IndX,Read), u(Nop,Imm,Read), u(Isb,IndX,Rmw),
    /* E4 */ e(Cpx,Zp,Read),  e(Sbc,Zp,Read),   e(Inc,Zp,Rmw),   u(Isb,Zp,Rmw),
    /* E8 */ e(Inx,Imp,None), e(Sbc,Imm,Read),  e(Nop,Imp,None), u(Sbc,Imm,Read),
    /* EC */ e(Cpx,Abs,Read), e(Sbc,Abs,Read),  e(Inc,Abs,Rmw),  u(Isb,Abs,Rmw),
    /* F0 */ e(Beq,Rel,None), e(Sbc,IndY,Read), u(Kil,Imp,None), u(Isb,IndY,Rmw),
    /* F4 */ u(Nop,ZpX,Read), e(Sbc,ZpX,Read),  e(Inc,ZpX,Rmw),  u(Isb,ZpX,Rmw),
    /* F8 */ e(Sed,Imp,None), e(Sbc,AbsY,Read), u(Nop,Imp,None), u(Isb,AbsY,Rmw),
    /* FC */ u(Nop,AbsX,Read),e(Sbc,AbsX,Read), e(Inc,AbsX,Rmw), u(Isb,AbsX,Rmw),
];

impl Op {
    pub fn mnemonic(self) -> &'static str {
        #[rustfmt::skip]
        let s = match self {
            Adc=>"ADC", And=>"AND", Asl=>"ASL", Bcc=>"BCC", Bcs=>"BCS", Beq=>"BEQ",
            Bit=>"BIT", Bmi=>"BMI", Bne=>"BNE", Bpl=>"BPL", Brk=>"BRK", Bvc=>"BVC",
            Bvs=>"BVS", Clc=>"CLC", Cld=>"CLD", Cli=>"CLI", Clv=>"CLV", Cmp=>"CMP",
            Cpx=>"CPX", Cpy=>"CPY", Dec=>"DEC", Dex=>"DEX", Dey=>"DEY", Eor=>"EOR",
            Inc=>"INC", Inx=>"INX", Iny=>"INY", Jmp=>"JMP", Jsr=>"JSR", Lda=>"LDA",
            Ldx=>"LDX", Ldy=>"LDY", Lsr=>"LSR", Nop=>"NOP", Ora=>"ORA", Pha=>"PHA",
            Php=>"PHP", Pla=>"PLA", Plp=>"PLP", Rol=>"ROL", Ror=>"ROR", Rti=>"RTI",
            Rts=>"RTS", Sbc=>"SBC", Sec=>"SEC", Sed=>"SED", Sei=>"SEI", Sta=>"STA",
            Stx=>"STX", Sty=>"STY", Tax=>"TAX", Tay=>"TAY", Tsx=>"TSX", Txa=>"TXA",
            Txs=>"TXS", Tya=>"TYA", Lax=>"LAX", Sax=>"SAX", Dcp=>"DCP", Isb=>"ISB",
            Slo=>"SLO", Rla=>"RLA", Sre=>"SRE", Rra=>"RRA", Anc=>"ANC", Alr=>"ALR",
            Arr=>"ARR", Axs=>"AXS", Xaa=>"XAA", Ahx=>"AHX", Shy=>"SHY", Shx=>"SHX",
            Tas=>"TAS", Las=>"LAS", Kil=>"KIL",
        };
        s
    }
}

impl Mode {
    /// Total instruction length in bytes, opcode included.
    #[allow(clippy::len_without_is_empty)] // Instruction byte width, not a collection.
    pub fn len(self) -> u16 {
        match self {
            Imp | Acc => 1,
            Imm | Zp | ZpX | ZpY | IndX | IndY | Rel => 2,
            Abs | AbsX | AbsY | Ind => 3,
        }
    }
}
