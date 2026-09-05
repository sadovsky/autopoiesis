use autopoiesis::isa::{Instruction, N_OPS};
use std::collections::HashSet;

#[test]
fn every_byte_decodes_and_reencodes_canonically() {
    for b in 0..=255u8 {
        let ins = Instruction::decode(b);
        let canon = ins.encode();
        // Same meaning after re-encoding…
        assert_eq!(Instruction::decode(canon), ins, "byte {b:#04x}");
        // …and the canonical form is a fixed point.
        assert_eq!(Instruction::decode(canon).encode(), canon, "byte {b:#04x}");
        // Canonical bytes only use valid opcodes and 3-bit direction operands.
        assert!((canon & 0x0F) < N_OPS, "byte {b:#04x} -> {canon:#04x}");
        assert!(canon >> 4 < 8, "byte {b:#04x} -> {canon:#04x}");
        if ins.dir().is_none() {
            assert_eq!(canon >> 4, 0, "operand-less op must have zero operand nibble");
        }
    }
}

#[test]
fn distinct_instructions_have_distinct_canonical_bytes() {
    let canon: HashSet<u8> = (0..=255u8).map(|b| Instruction::decode(b).encode()).collect();
    let distinct: HashSet<Instruction> = (0..=255u8).map(Instruction::decode).collect();
    assert_eq!(canon.len(), distinct.len());
    // 8 directional ops × 8 directions + Nop + SetTag + Halt.
    assert_eq!(distinct.len(), 8 * 8 + 3);
}

#[test]
fn invalid_opcodes_are_nop() {
    for op in N_OPS..16 {
        for hi in 0..16u8 {
            assert_eq!(Instruction::decode(op | (hi << 4)), Instruction::Nop);
        }
    }
    assert_eq!(Instruction::decode(0x24), Instruction::Repair(2));
    assert_eq!(Instruction::decode(0xA4), Instruction::Repair(2)); // 4th operand bit ignored
    assert_eq!(format!("{}", Instruction::Repair(2)), "Repair(E)");
}
