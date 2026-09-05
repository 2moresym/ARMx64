use crate::arch::aarch64::{self, A64Inst};
use crate::ir::{Block, IRInst, Opcode, Value};
use yaxpeax_arm::armv8::a64::{Opcode as A64Opcode, Operand, Instruction};

/// Lift one decoded guest instruction into ARMx64 IR.
///
/// Decodes the AArch64 instruction and translates it into IR instructions
/// added to the provided basic block.
#[inline]
pub fn lift_one(inst: A64Inst, block: &mut Block) {
    let decoded = match aarch64::decode(inst) {
        Ok(d) => d,
        Err(_) => {
            // Represent decode failure explicitly in IR or as an unsupported/trap inst
            block.push(IRInst {
                opcode: Opcode::Nop,
                flags: 0,
                a: Value(0),
                b: Value(0),
                c: Value(0),
            });
            return;
        }
    };

    let ir_opcode = match decoded.opcode {
        A64Opcode::NOP => Opcode::Nop,
        A64Opcode::ADD | A64Opcode::ADDS => Opcode::Add,
        A64Opcode::SUB | A64Opcode::SUBS => Opcode::Sub,
        A64Opcode::AND | A64Opcode::ANDS => Opcode::And,
        A64Opcode::ORR => Opcode::Orr,
        A64Opcode::EOR => Opcode::Eor,
        A64Opcode::MOV => Opcode::Mov,
        A64Opcode::LSL | A64Opcode::LSR => Opcode::Shift,
        _ => {
            // Unsupported instruction - push Nop or handle safely without corrupting guest semantics
            block.push(IRInst {
                opcode: Opcode::Nop,
                flags: 0,
                a: Value(0),
                b: Value(0),
                c: Value(0),
            });
            return;
        }
    };

    block.push(IRInst {
        opcode: ir_opcode,
        flags: 0,
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
        let inst = A64Inst(0xd503201f);
        lift_one(inst, &mut block);
        assert_eq!(block.insts.len(), 1);
        assert_eq!(block.insts[0].opcode, Opcode::Nop);
    }
}
