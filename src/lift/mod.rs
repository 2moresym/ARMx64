use crate::arch::aarch64::{self, A64Inst};
use crate::ir::{Block, IRInst, Opcode, Value, FLAG_WRITES_NZCV};
use yaxpeax_arm::armv8::a64::Opcode as A64Opcode;

/// Lift one decoded guest instruction into ARMx64 IR.
///
/// Unsupported or undecodable instructions are represented explicitly as
/// `Opcode::Unsupported`; they must never silently become NOPs.
#[inline]
pub fn lift_one(inst: A64Inst, block: &mut Block) {
    let decoded = match aarch64::decode(inst) {
        Ok(d) => d,
        Err(_) => {
            block.push(IRInst {
                opcode: Opcode::Unsupported,
                flags: 0,
                a: Value(inst.0),
                b: Value(0),
                c: Value(0),
            });
            return;
        }
    };

    let (ir_opcode, flags) = match decoded.opcode {
        A64Opcode::NOP => (Opcode::Nop, 0),
        A64Opcode::ADD => (Opcode::Add, 0),
        A64Opcode::ADDS => (Opcode::Add, FLAG_WRITES_NZCV),
        A64Opcode::SUB => (Opcode::Sub, 0),
        A64Opcode::SUBS => (Opcode::Sub, FLAG_WRITES_NZCV),
        A64Opcode::AND => (Opcode::And, 0),
        A64Opcode::ANDS => (Opcode::And, FLAG_WRITES_NZCV),
        A64Opcode::ORR => (Opcode::Orr, 0),
        A64Opcode::EOR => (Opcode::Eor, 0),
        A64Opcode::MOV => (Opcode::Mov, 0),
        A64Opcode::LSL | A64Opcode::LSR => (Opcode::Shift, 0),
        _ => {
            block.push(IRInst {
                opcode: Opcode::Unsupported,
                flags: 0,
                a: Value(inst.0),
                b: Value(0),
                c: Value(0),
            });
            return;
        }
    };

    block.push(IRInst {
        opcode: ir_opcode,
        flags,
        a: Value(0),
        b: Value(0),
        c: Value(0),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lift_nop() {
        let mut block = Block::new();
        lift_one(A64Inst(0xd503201f), &mut block);
        assert_eq!(block.insts.len(), 1);
        assert_eq!(block.insts[0].opcode, Opcode::Nop);
    }

    #[test]
    fn test_unsupported_is_not_nop() {
        let mut block = Block::new();
        lift_one(A64Inst(0xd61f0000), &mut block);
        assert_eq!(block.insts.len(), 1);
        assert_eq!(block.insts[0].opcode, Opcode::Unsupported);
        assert_eq!(block.insts[0].a, Value(0xd61f0000));
    }

    #[test]
    fn test_flag_setting_opcode_is_preserved() {
        let mut block = Block::new();
        lift_one(A64Inst(0xab000000), &mut block);
        assert_eq!(block.insts.len(), 1);
        assert_eq!(block.insts[0].opcode, Opcode::Add);
        assert_ne!(block.insts[0].flags & FLAG_WRITES_NZCV, 0);
    }
}
