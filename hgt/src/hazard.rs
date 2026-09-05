//! The environment: one stressor per tick, and the genes that answer it.
//!
//! The stressor schedule is a *pure function of `(seed, tick)`*, so every node derives it
//! independently and there is no environment server to be the secret centre of a
//! supposedly decentralised network. A run over real sockets and a run in one process
//! face exactly the same sequence.
//!
//! A stressor of kind `k` is a payload word; the correct response is
//! `rotate_left(payload ^ key_k, rot_k)` with `(key_k, rot_k)` derived from the seed. A
//! node survives the tick if one of its genes *computes* that. Resistance is therefore a
//! program, not a flag — `resistance_gene` compiles one, and the compiled gene spends
//! most of its length building the 32-bit key a nibble at a time, which is exactly why a
//! single mutated byte in the wrong place destroys it.

use crate::config::HgtConfig;
use crate::isa::Instruction as I;

/// The stressor posed on one tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Challenge {
    pub kind: u8,
    pub payload: u32,
    /// What a gene has to emit to survive it.
    pub answer: u32,
}

/// The stressor schedule and its hidden transforms.
#[derive(Clone, Debug)]
pub struct Environment {
    seed: u64,
    kinds: u8,
    epoch_ticks: u32,
}

impl Environment {
    pub fn new(cfg: &HgtConfig, seed: u64) -> Environment {
        Environment { seed, kinds: cfg.hazard_kinds.max(1), epoch_ticks: cfg.epoch_ticks.max(1) }
    }

    /// Epochs are the unit of change: within one, the stressor kind is constant.
    pub fn epoch_at(&self, tick: u32) -> u32 {
        tick / self.epoch_ticks
    }

    pub fn kind_at(&self, tick: u32) -> u8 {
        (self.epoch_at(tick) % self.kinds as u32) as u8
    }

    pub fn kinds(&self) -> u8 {
        self.kinds
    }

    /// The (key, rotation) pair a gene has to know to answer stressor `kind`.
    pub fn secret(&self, kind: u8) -> (u32, u8) {
        let h = splitmix64(self.seed ^ 0x5354_5245_5353_0000 ^ kind as u64);
        (h as u32, (1 + (h >> 40) % 15) as u8)
    }

    pub fn payload_at(&self, tick: u32) -> u32 {
        splitmix64(self.seed ^ (tick as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)) as u32
    }

    pub fn answer(&self, kind: u8, payload: u32) -> u32 {
        let (key, rot) = self.secret(kind);
        (payload ^ key).rotate_left(rot as u32)
    }

    pub fn challenge_at(&self, tick: u32) -> Challenge {
        let kind = self.kind_at(tick);
        let payload = self.payload_at(tick);
        Challenge { kind, payload, answer: self.answer(kind, payload) }
    }

    /// A gene that answers stressor `kind`, as bytes. This is the only place in the
    /// sandbox where working code is written rather than copied or mutated.
    pub fn resistance_gene(&self, kind: u8) -> Vec<u8> {
        let (key, rot) = self.secret(kind);
        compile_resistance(key, rot)
    }
}

/// Compile `rotate_left(payload ^ key, rot)`: build the key nibble by nibble into `acc`,
/// park it in `aux`, load the payload, fold, rotate, emit.
pub fn compile_resistance(key: u32, rot: u8) -> Vec<u8> {
    let nib = |i: u32| ((key >> (28 - 4 * i)) & 0xF) as u8;
    let mut prog = vec![I::Imm(nib(0))];
    for i in 1..8 {
        prog.push(I::Shl(4));
        prog.push(I::Or(nib(i)));
    }
    prog.push(I::Swap);
    prog.push(I::Payload);
    prog.push(I::XorAux);
    prog.push(I::Rotl(rot & 0x0F));
    prog.push(I::Emit);
    prog.iter().map(|i| i.encode()).collect()
}

/// SplitMix64 — a fixed mixing function, not an RNG stream, so the schedule can be
/// evaluated at any tick by any node without shared state.
pub fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm;

    #[test]
    fn a_compiled_gene_answers_its_own_stressor_and_no_other() {
        let cfg = HgtConfig::default();
        let env = Environment::new(&cfg, 7);
        for kind in 0..cfg.hazard_kinds {
            let code = env.resistance_gene(kind);
            assert!(code.len() <= 64, "gene is {} bytes", code.len());
            for tick in [0u32, 1, 977, 30_000] {
                let payload = env.payload_at(tick);
                let out = vm::run(&code, payload, kind, cfg.vm_budget);
                assert_eq!(out.answer, Some(env.answer(kind, payload)), "kind {kind} tick {tick}");
                for other in 0..cfg.hazard_kinds {
                    if other != kind {
                        assert_ne!(
                            out.answer,
                            Some(env.answer(other, payload)),
                            "gene for {kind} also answered {other}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_schedule_shifts_on_epoch_boundaries_and_depends_on_the_seed() {
        let cfg = HgtConfig { epoch_ticks: 100, hazard_kinds: 3, ..HgtConfig::default() };
        let env = Environment::new(&cfg, 1);
        assert_eq!(env.kind_at(0), 0);
        assert_eq!(env.kind_at(99), 0);
        assert_eq!(env.kind_at(100), 1);
        assert_eq!(env.kind_at(300), 0, "kinds cycle");
        let other = Environment::new(&cfg, 2);
        assert_ne!(env.secret(0), other.secret(0), "the answer must depend on the run seed");
        assert_ne!(env.payload_at(5), other.payload_at(5));
    }

    #[test]
    fn one_mutated_byte_in_the_key_destroys_resistance() {
        let cfg = HgtConfig::default();
        let env = Environment::new(&cfg, 3);
        let code = env.resistance_gene(1);
        let payload = env.payload_at(10);
        for i in 0..code.len() {
            let mut broken = code.clone();
            broken[i] = broken[i].wrapping_add(0x10); // change an operand nibble
            let out = vm::run(&broken, payload, 1, cfg.vm_budget);
            if out.answer == Some(env.answer(1, payload)) {
                // Only a no-op operand change may survive (e.g. Shl on an op with no
                // operand); every key nibble must matter.
                assert!(
                    matches!(
                        crate::isa::Instruction::decode(code[i]),
                        crate::isa::Instruction::Swap
                            | crate::isa::Instruction::Payload
                            | crate::isa::Instruction::XorAux
                            | crate::isa::Instruction::Emit
                    ),
                    "byte {i} ({:#04x}) could be changed without breaking the gene",
                    code[i]
                );
            }
        }
    }
}
