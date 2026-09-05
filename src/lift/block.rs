use crate::arch::aarch64::{self, A64Inst};
use crate::ir::Block;
use super::lift_one;

/// Lift one guest basic block from little-endian AArch64 words.
#[inline]
pub fn lift_block(words: &[u32], block: &mut Block) -> usize {
    let mut consumed = 0;
    for &word in words {
        let inst = A64Inst(word);
        let terminal = match aarch64::decode(inst) {
            Ok(decoded) => matches_terminal(&decoded.opcode),
            Err(_) => true,
        };
        lift_one(inst, block);
        consumed += 1;
        if terminal { break; }
    }
    consumed
}

/// Start a new basic block with explicit guest PC metadata and lift into it.
#[inline]
pub fn lift_block_at(words: &[u32], guest_pc: u64, block: &mut Block) -> usize {
    block.guest_pc = guest_pc;
    lift_block(words, block)
}

#[inline]
fn matches_terminal(opcode: &yaxpeax_arm::armv8::a64::Opcode) -> bool {
    use yaxpeax_arm::armv8::a64::Opcode::*;
    matches!(opcode, B | BR | Bcc(_) | BL | BLR | CBZ | CBNZ | RET)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Opcode;

    #[test]
    fn block_preserves_pc_and_stops_after_branch() {
        let mut block = Block::new();
        let consumed = lift_block_at(&[0xd503201f, 0x14000000, 0xd503201f], 0x1000, &mut block);
        assert_eq!(consumed, 2);
        assert_eq!(block.guest_pc, 0x1000);
        assert_eq!(block.byte_len(), 8);
        assert_eq!(block.insts[0].opcode, Opcode::Nop);
        assert_eq!(block.insts[1].opcode, Opcode::Branch);
    }
}
