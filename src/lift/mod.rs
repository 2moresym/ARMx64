use crate::arch::aarch64::{self, A64Inst};
use crate::ir::{Block, IRInst, Opcode, Value};
use yaxpeax_arm::armv8::a64::Mnemonic;

/// Lift one decoded guest instruction into ARMx64 IR.
///
/// Decodes the AArch64 instruction and translates it into IR instructions
/// added to the provided basic block.
#[inline]
pub fn lift_one(inst: A64Inst, block: &mut Block) {
    let decoded = match aarch64::decode(inst) {
        Ok(d) => d,
        Err(_) => {
            // Fallback or trap for decode failures
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

    let opcode = match decoded.mnemonic() {
        Mnemonic::NOP => Opcode::Nop,
        _ => Opcode::Nop, // Extend with more mappings as required by the milestone
    };

    block.push(IRInst {
        opcode,
        flags: 0,
        a: Value(0),
        b: Value(0),
        c: Value(0),
    });
}
