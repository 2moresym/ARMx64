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
    fn block_stops_after_branch() {
        let mut block = Block::new();
        let consumed = lift_block(&[0xd503201f, 0x14000000, 0xd503201f], &mut block);
        assert_eq!(consumed, 2);
        assert_eq!(block.insts.len(), 2);
        assert_eq!(block.insts[0].opcode, Opcode::Nop);
        assert_eq!(block.insts[1].opcode, Opcode::Branch);
    }
}
