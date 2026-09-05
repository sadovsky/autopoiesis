//! Phase 1 acceptance: every byte is a program. Genes travel over a network and get
//! mutated in flight, so there is no such thing as an "invalid" gene byte — only one
//! that means something different.

use hgt::isa::{Instruction, N_OPS, glyphs};
use std::collections::HashSet;

#[test]
fn every_byte_decodes_and_encoding_is_a_fixed_point() {
    for b in 0..=u8::MAX {
        let ins = Instruction::decode(b);
        let canon = ins.encode();
        assert_eq!(Instruction::decode(canon), ins, "byte {b:#04x} changed meaning when canonicalised");
        assert_eq!(Instruction::decode(canon).encode(), canon, "encode(decode(x)) is not idempotent at {b:#04x}");
        assert!(ins.opcode() < N_OPS, "byte {b:#04x} decoded to opcode {}", ins.opcode());
    }
}

#[test]
fn operandless_ops_canonicalise_their_high_nibble_away() {
    for n in 0..16u8 {
        let byte = (n << 4) | Instruction::Emit.encode();
        assert_eq!(Instruction::decode(byte), Instruction::Emit);
        assert_eq!(Instruction::decode(byte).encode(), Instruction::Emit.encode());
    }
    // Directional ops keep theirs.
    assert_eq!(Instruction::decode(Instruction::Imm(9).encode()), Instruction::Imm(9));
}

#[test]
fn every_op_has_a_distinct_glyph_for_the_renderer() {
    let mut seen = HashSet::new();
    for b in 0..=u8::MAX {
        let ins = Instruction::decode(b);
        seen.insert((ins.glyph(), ins.opcode()));
    }
    let glyph_count = seen.iter().map(|(g, _)| *g).collect::<HashSet<char>>().len();
    assert_eq!(glyph_count, N_OPS as usize, "glyphs collide: {seen:?}");
    assert_eq!(glyphs(&[Instruction::Payload.encode(), Instruction::Emit.encode()]), "PE");
}
