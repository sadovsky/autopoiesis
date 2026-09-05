//! The instruction set. Deliberately tiny (11 ops) so that random bytes are mostly
//! valid programs and single-byte mutations are meaningful.
//!
//! Encoding: low nibble = opcode, high nibble = operand. For directional ops the
//! operand's low three bits select a compass direction (see `grid::DIRS`); the fourth
//! bit is ignored. Opcodes 11..=15 are invalid and decode to `Nop`. Every byte
//! therefore decodes, and `encode(decode(b))` yields a canonical byte with the same
//! meaning.

use crate::config::Costs;
use crate::grid::DIR_NAMES;
use std::fmt;

/// Compass direction 0..8 (N, NE, E, SE, S, SW, W, NW).
pub type Dir = u8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Instruction {
    /// Do nothing.
    Nop,
    /// `ip` jumps to the neighbour in direction `d`: from next tick on the cell
    /// executes that neighbour's byte. Execution flows spatially.
    MoveIp(Dir),
    /// `reg = neighbor(d).instr`
    Load(Dir),
    /// `neighbor(d).instr = reg`
    Store(Dir),
    /// `neighbor(d).instr = self.instr; neighbor(d).tag = self.tag`.
    /// The only way a pattern fights noise.
    Repair(Dir),
    /// `reg = (neighbor(d).instr == self.instr) as u8`
    Cmp(Dir),
    /// If `reg == 0`, `ip` jumps to the neighbour in direction `d`.
    JmpIfZero(Dir),
    /// Pull up to `absorb_rate` energy from neighbour `d`.
    Absorb(Dir),
    /// Push up to `share_rate` energy to neighbour `d`.
    Share(Dir),
    /// `tag = reg`
    SetTag,
    /// Dormant (ip frozen, zero cost) until energy exceeds `halt_threshold`.
    Halt,
}

pub const OP_NOP: u8 = 0;
pub const OP_MOVE_IP: u8 = 1;
pub const OP_LOAD: u8 = 2;
pub const OP_STORE: u8 = 3;
pub const OP_REPAIR: u8 = 4;
pub const OP_CMP: u8 = 5;
pub const OP_JMP_IF_ZERO: u8 = 6;
pub const OP_ABSORB: u8 = 7;
pub const OP_SHARE: u8 = 8;
pub const OP_SET_TAG: u8 = 9;
pub const OP_HALT: u8 = 10;
pub const N_OPS: u8 = 11;

impl Instruction {
    #[inline]
    pub fn decode(byte: u8) -> Instruction {
        let d = (byte >> 4) & 7;
        match byte & 0x0F {
            OP_NOP => Instruction::Nop,
            OP_MOVE_IP => Instruction::MoveIp(d),
            OP_LOAD => Instruction::Load(d),
            OP_STORE => Instruction::Store(d),
            OP_REPAIR => Instruction::Repair(d),
            OP_CMP => Instruction::Cmp(d),
            OP_JMP_IF_ZERO => Instruction::JmpIfZero(d),
            OP_ABSORB => Instruction::Absorb(d),
            OP_SHARE => Instruction::Share(d),
            OP_SET_TAG => Instruction::SetTag,
            OP_HALT => Instruction::Halt,
            _ => Instruction::Nop,
        }
    }

    /// Canonical byte: operand nibble is `d & 7` for directional ops and 0 otherwise.
    #[inline]
    pub fn encode(self) -> u8 {
        let enc = |op: u8, d: Dir| op | ((d & 7) << 4);
        match self {
            Instruction::Nop => OP_NOP,
            Instruction::MoveIp(d) => enc(OP_MOVE_IP, d),
            Instruction::Load(d) => enc(OP_LOAD, d),
            Instruction::Store(d) => enc(OP_STORE, d),
            Instruction::Repair(d) => enc(OP_REPAIR, d),
            Instruction::Cmp(d) => enc(OP_CMP, d),
            Instruction::JmpIfZero(d) => enc(OP_JMP_IF_ZERO, d),
            Instruction::Absorb(d) => enc(OP_ABSORB, d),
            Instruction::Share(d) => enc(OP_SHARE, d),
            Instruction::SetTag => OP_SET_TAG,
            Instruction::Halt => OP_HALT,
        }
    }

    /// Opcode number 0..N_OPS.
    #[inline]
    pub fn opcode(self) -> u8 {
        self.encode() & 0x0F
    }

    /// Direction operand, if the op has one.
    #[inline]
    pub fn dir(self) -> Option<Dir> {
        match self {
            Instruction::MoveIp(d)
            | Instruction::Load(d)
            | Instruction::Store(d)
            | Instruction::Repair(d)
            | Instruction::Cmp(d)
            | Instruction::JmpIfZero(d)
            | Instruction::Absorb(d)
            | Instruction::Share(d) => Some(d),
            Instruction::Nop | Instruction::SetTag | Instruction::Halt => None,
        }
    }

    #[inline]
    pub fn cost(self, costs: &Costs) -> u16 {
        match self {
            Instruction::Nop => costs.nop,
            Instruction::MoveIp(_) => costs.move_ip,
            Instruction::Load(_) => costs.load,
            Instruction::Store(_) => costs.store,
            Instruction::Repair(_) => costs.repair,
            Instruction::Cmp(_) => costs.cmp,
            Instruction::JmpIfZero(_) => costs.jmp_if_zero,
            Instruction::Absorb(_) => costs.absorb,
            Instruction::Share(_) => costs.share,
            Instruction::SetTag => costs.set_tag,
            Instruction::Halt => costs.halt,
        }
    }

    /// One-letter mnemonic for the renderer.
    pub fn glyph(self) -> char {
        match self {
            Instruction::Nop => '.',
            Instruction::MoveIp(_) => 'm',
            Instruction::Load(_) => 'l',
            Instruction::Store(_) => 's',
            Instruction::Repair(_) => 'R',
            Instruction::Cmp(_) => 'c',
            Instruction::JmpIfZero(_) => 'j',
            Instruction::Absorb(_) => 'A',
            Instruction::Share(_) => 'S',
            Instruction::SetTag => 't',
            Instruction::Halt => 'h',
        }
    }
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Instruction::Nop => "Nop",
            Instruction::MoveIp(_) => "MoveIp",
            Instruction::Load(_) => "Load",
            Instruction::Store(_) => "Store",
            Instruction::Repair(_) => "Repair",
            Instruction::Cmp(_) => "Cmp",
            Instruction::JmpIfZero(_) => "JmpIfZero",
            Instruction::Absorb(_) => "Absorb",
            Instruction::Share(_) => "Share",
            Instruction::SetTag => "SetTag",
            Instruction::Halt => "Halt",
        };
        match self.dir() {
            Some(d) => write!(f, "{}({})", name, DIR_NAMES[(d & 7) as usize]),
            None => write!(f, "{}", name),
        }
    }
}
