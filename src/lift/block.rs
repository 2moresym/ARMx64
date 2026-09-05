use crate::arch::aarch64::{self, A64Inst};
use crate::ir::Block;
use super::lift_one_at;

/// Lift one guest basic block from little-endian AArch64 words.
#[inline]
pub fn lift_block(words: &[u32], block: &mut Block) -> usize {
    lift_block_at(words, block.guest_pc, block)
}

/// Start a new basic block with explicit guest PC metadata and lift into it.
#[inline]
pub fn lift_block_at(words: &[u32], guest_pc: u64, block: &mut Block) -> usize {
    block.guest_pc = guest_pc;
    let mut consumed = 0;
    for &word in words {
        let inst = A64Inst(word);
        let inst_pc = guest_pc + consumed as u64 * 4;
        let terminal = match aarch64::decode(inst) {
            Ok(decoded) => matches_terminal(&decoded.opcode),
            Err(_) => true,
        };
        lift_one_at(inst, inst_pc, block);
        consumed += 1;
        if terminal { break; }
    }
    consumed
}

#[inline]
fn matches_terminal(opcode: &yaxpeax_arm::armv8::a64::Opcode) -> bool {
    use yaxpeax_arm::armv8::a64::Opcode::*;
    matches!(opcode, B | BR | Bcc(_) | BL | BLR | CBZ | CBNZ | RET)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Opcode, Operand};

    #[test]
    fn block_preserves_pc_and_stops_after_branch() {
        let mut block = Block::new();
        let consumed = lift_block_at(&[0xd503201f, 0x14000000, 0xd503201f], 0x1000, &mut block);
        assert_eq!(consumed, 2);
        assert_eq!(block.guest_pc, 0x1000);
        assert_eq!(block.byte_len(), 8);
        assert_eq!(block.insts[0].opcode, Opcode::Nop);
        assert_eq!(block.insts[1].opcode, Opcode::Branch);
        assert_eq!(block.insts[1].a, Operand::GuestPc(0x1000));
    }
}
