//! The gene interpreter: an accumulator machine with a hard step budget.
//!
//! Genes arrive over the network from other machines, and most of them are mutated
//! copies of something that once worked. The interpreter is therefore written so that
//! *any* byte string is a safe thing to be handed: every byte decodes (`isa.rs`), every
//! arithmetic op wraps, jumps outside the code halt, and the step budget bounds the run.
//! There is no way for a received gene to panic, hang, or reach anything outside these
//! four `u32`s — which is the whole reason genes are bytecode here and not, say, shell.

use crate::isa::Instruction;

/// Why a gene stopped running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// Executed `Emit`.
    Emitted,
    /// Executed `Halt`.
    Halted,
    /// `pc` ran past the end of the code, or a jump left it.
    RanOff,
    /// Used its whole step budget.
    Budget,
}

/// The result of running a gene against one stressor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// The value passed to `Emit`, if the gene emitted one.
    pub answer: Option<u32>,
    /// Instructions executed. Metabolism is charged on this.
    pub steps: u32,
    pub stop: Stop,
}

/// Run `code` against a stressor. Total work is bounded by `budget`.
pub fn run(code: &[u8], payload: u32, kind: u8, budget: u32) -> Outcome {
    let mut acc: u32 = 0;
    let mut aux: u32 = 0;
    let mut pc: usize = 0;
    let mut steps: u32 = 0;

    loop {
        if pc >= code.len() {
            return Outcome { answer: None, steps, stop: Stop::RanOff };
        }
        if steps >= budget {
            return Outcome { answer: None, steps, stop: Stop::Budget };
        }
        steps += 1;
        let mut next = pc + 1;
        match Instruction::decode(code[pc]) {
            Instruction::Nop => {}
            Instruction::Imm(n) => acc = n as u32,
            Instruction::Shl(n) => acc = acc.wrapping_shl(n as u32),
            Instruction::Or(n) => acc |= n as u32,
            Instruction::Xor(n) => acc ^= n as u32,
            Instruction::Rotl(n) => acc = acc.rotate_left(n as u32),
            Instruction::Add(n) => acc = acc.wrapping_add(n as u32),
            Instruction::XorAux => acc ^= aux,
            Instruction::AddAux => acc = acc.wrapping_add(aux),
            Instruction::Swap => std::mem::swap(&mut acc, &mut aux),
            Instruction::Payload => acc = payload,
            Instruction::Kind => acc = kind as u32,
            Instruction::JmpIfZero(n) => {
                if acc == 0 {
                    next = n as usize;
                }
            }
            Instruction::Emit => {
                return Outcome { answer: Some(acc), steps, stop: Stop::Emitted };
            }
            Instruction::Halt => {
                return Outcome { answer: None, steps, stop: Stop::Halted };
            }
        }
        pc = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::Instruction as I;

    fn asm(prog: &[I]) -> Vec<u8> {
        prog.iter().map(|i| i.encode()).collect()
    }

    #[test]
    fn a_gene_can_read_and_transform_the_payload() {
        // acc = payload; acc ^= 0xF; acc = rotl(acc, 3); emit
        let code = asm(&[I::Payload, I::Xor(0xF), I::Rotl(3), I::Emit]);
        let out = run(&code, 0x1234_5678, 0, 64);
        assert_eq!(out.answer, Some((0x1234_5678u32 ^ 0xF).rotate_left(3)));
        assert_eq!(out.stop, Stop::Emitted);
        assert_eq!(out.steps, 4);
    }

    #[test]
    fn a_gene_can_branch_on_the_hazard_kind() {
        // Answer only for kind 0: acc = kind; jz 3; halt; (3) acc = payload; emit
        let code = asm(&[I::Kind, I::JmpIfZero(3), I::Halt, I::Payload, I::Emit]);
        assert_eq!(run(&code, 7, 0, 64).answer, Some(7));
        assert_eq!(run(&code, 7, 1, 64).answer, None);
    }

    #[test]
    fn an_infinite_loop_is_stopped_by_the_budget() {
        // acc = 0; jz 0 — jumps to itself forever.
        let code = asm(&[I::Imm(0), I::JmpIfZero(0)]);
        let out = run(&code, 0, 0, 32);
        assert_eq!(out.stop, Stop::Budget);
        assert_eq!(out.steps, 32);
        assert_eq!(out.answer, None);
    }

    #[test]
    fn empty_code_and_jumps_off_the_end_run_off_rather_than_panic() {
        assert_eq!(run(&[], 1, 0, 8).stop, Stop::RanOff);
        let code = asm(&[I::Imm(0), I::JmpIfZero(15)]);
        assert_eq!(run(&code, 1, 0, 8).stop, Stop::RanOff);
    }
}
