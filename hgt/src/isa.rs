//! The gene instruction set. A gene is a program, so a gene has to *be* bytes that mean
//! something: 15 ops, one byte each, low nibble = opcode, high nibble = a 0..15 operand.
//!
//! One byte per instruction (rather than an opcode plus an immediate byte) is the whole
//! design decision. It means every possible byte string is a valid program, a single
//! byte flip is a small edit rather than a frame shift, and a truncated transfer is a
//! shorter program rather than garbage — which is what makes mutation and partial
//! uptake meaningful instead of merely fatal. The price is that constants have to be
//! built a nibble at a time (`Imm`, `Shl`, `Or`), and that is exactly what a compiled
//! resistance gene spends most of its length doing.
//!
//! The machine is an accumulator machine: `acc` and `aux`, both `u32`, plus the hazard
//! payload and kind as loadable inputs. See `vm.rs`.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Instruction {
    /// Do nothing.
    Nop,
    /// `acc = n`
    Imm(u8),
    /// `acc <<= n` (wrapping; `n = 0` is a no-op)
    Shl(u8),
    /// `acc |= n`
    Or(u8),
    /// `acc ^= n`
    Xor(u8),
    /// `acc = acc.rotate_left(n)`
    Rotl(u8),
    /// `acc = acc.wrapping_add(n)`
    Add(u8),
    /// `acc ^= aux`
    XorAux,
    /// `acc = acc.wrapping_add(aux)`
    AddAux,
    /// `swap(acc, aux)`
    Swap,
    /// `acc = payload` — the stressor this gene is being asked about.
    Payload,
    /// `acc = kind` — which stressor it is.
    Kind,
    /// If `acc == 0`, `pc = n`. The only control flow; jumps land in the first 16 bytes.
    JmpIfZero(u8),
    /// Answer with `acc` and stop.
    Emit,
    /// Stop without answering.
    Halt,
}

pub const OP_NOP: u8 = 0;
pub const OP_IMM: u8 = 1;
pub const OP_SHL: u8 = 2;
pub const OP_OR: u8 = 3;
pub const OP_XOR: u8 = 4;
pub const OP_ROTL: u8 = 5;
pub const OP_ADD: u8 = 6;
pub const OP_XOR_AUX: u8 = 7;
pub const OP_ADD_AUX: u8 = 8;
pub const OP_SWAP: u8 = 9;
pub const OP_PAYLOAD: u8 = 10;
pub const OP_KIND: u8 = 11;
pub const OP_JMP_IF_ZERO: u8 = 12;
pub const OP_EMIT: u8 = 13;
pub const OP_HALT: u8 = 14;
pub const N_OPS: u8 = 15;

impl Instruction {
    /// Every byte decodes. Opcode 15 is unused and reads as `Nop`.
    #[inline]
    pub fn decode(byte: u8) -> Instruction {
        let n = byte >> 4;
        match byte & 0x0F {
            OP_IMM => Instruction::Imm(n),
            OP_SHL => Instruction::Shl(n),
            OP_OR => Instruction::Or(n),
            OP_XOR => Instruction::Xor(n),
            OP_ROTL => Instruction::Rotl(n),
            OP_ADD => Instruction::Add(n),
            OP_XOR_AUX => Instruction::XorAux,
            OP_ADD_AUX => Instruction::AddAux,
            OP_SWAP => Instruction::Swap,
            OP_PAYLOAD => Instruction::Payload,
            OP_KIND => Instruction::Kind,
            OP_JMP_IF_ZERO => Instruction::JmpIfZero(n),
            OP_EMIT => Instruction::Emit,
            OP_HALT => Instruction::Halt,
            _ => Instruction::Nop,
        }
    }

    /// Canonical byte: the operand nibble is 0 for ops that have no operand.
    #[inline]
    pub fn encode(self) -> u8 {
        let enc = |op: u8, n: u8| op | ((n & 0x0F) << 4);
        match self {
            Instruction::Nop => OP_NOP,
            Instruction::Imm(n) => enc(OP_IMM, n),
            Instruction::Shl(n) => enc(OP_SHL, n),
            Instruction::Or(n) => enc(OP_OR, n),
            Instruction::Xor(n) => enc(OP_XOR, n),
            Instruction::Rotl(n) => enc(OP_ROTL, n),
            Instruction::Add(n) => enc(OP_ADD, n),
            Instruction::XorAux => OP_XOR_AUX,
            Instruction::AddAux => OP_ADD_AUX,
            Instruction::Swap => OP_SWAP,
            Instruction::Payload => OP_PAYLOAD,
            Instruction::Kind => OP_KIND,
            Instruction::JmpIfZero(n) => enc(OP_JMP_IF_ZERO, n),
            Instruction::Emit => OP_EMIT,
            Instruction::Halt => OP_HALT,
        }
    }

    /// Opcode number, 0..N_OPS.
    #[inline]
    pub fn opcode(self) -> u8 {
        self.encode() & 0x0F
    }

    /// Operand, if the op has one.
    #[inline]
    pub fn operand(self) -> Option<u8> {
        match self {
            Instruction::Imm(n)
            | Instruction::Shl(n)
            | Instruction::Or(n)
            | Instruction::Xor(n)
            | Instruction::Rotl(n)
            | Instruction::Add(n)
            | Instruction::JmpIfZero(n) => Some(n),
            _ => None,
        }
    }

    /// One-letter mnemonic for the renderer and for gene dumps.
    pub fn glyph(self) -> char {
        match self {
            Instruction::Nop => '.',
            Instruction::Imm(_) => 'i',
            Instruction::Shl(_) => '<',
            Instruction::Or(_) => '|',
            Instruction::Xor(_) => '^',
            Instruction::Rotl(_) => '@',
            Instruction::Add(_) => '+',
            Instruction::XorAux => 'X',
            Instruction::AddAux => 'A',
            Instruction::Swap => 's',
            Instruction::Payload => 'P',
            Instruction::Kind => 'K',
            Instruction::JmpIfZero(_) => 'j',
            Instruction::Emit => 'E',
            Instruction::Halt => 'h',
        }
    }
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Instruction::Nop => "Nop",
            Instruction::Imm(_) => "Imm",
            Instruction::Shl(_) => "Shl",
            Instruction::Or(_) => "Or",
            Instruction::Xor(_) => "Xor",
            Instruction::Rotl(_) => "Rotl",
            Instruction::Add(_) => "Add",
            Instruction::XorAux => "XorAux",
            Instruction::AddAux => "AddAux",
            Instruction::Swap => "Swap",
            Instruction::Payload => "Payload",
            Instruction::Kind => "Kind",
            Instruction::JmpIfZero(_) => "JmpIfZero",
            Instruction::Emit => "Emit",
            Instruction::Halt => "Halt",
        };
        match self.operand() {
            Some(n) => write!(f, "{name}({n})"),
            None => write!(f, "{name}"),
        }
    }
}

/// Disassemble a gene into one-letter glyphs — how a gene appears in the renderer.
pub fn glyphs(code: &[u8]) -> String {
    code.iter().map(|&b| Instruction::decode(b).glyph()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_byte_decodes_and_reencodes_canonically() {
        for b in 0..=u8::MAX {
            let ins = Instruction::decode(b);
            let canon = ins.encode();
            assert_eq!(Instruction::decode(canon), ins, "byte {b:#04x} lost meaning");
            assert!(ins.opcode() < N_OPS, "byte {b:#04x} decoded to opcode {}", ins.opcode());
        }
    }
}
