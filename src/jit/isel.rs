use crate::ir::{Block, IRInst, Opcode, Operand, RegWidth};

/// Compiler-internal hint: use x86's immediate form when the operand can be
/// represented without changing guest semantics. The low bit remains reserved
/// for ARM NZCV writes; this hint intentionally lives in the higher flag bits.
pub const FLAG_X86_IMM_FORM: u16 = 1 << 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstForm {
    Generic,
    Immediate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub opcode: Opcode,
    pub form: InstForm,
    pub cost: u8,
}

/// Lightweight target-aware instruction selector.
///
/// Selection is deliberately separate from semantic optimization: the IR still
/// describes the guest operation, while this pass records the cheapest safe
/// x86-64-v2 lowering available to the current emitter.
#[derive(Debug, Default, Clone, Copy)]
pub struct InstructionSelector {
    pub immediate_forms: u32,
}

impl InstructionSelector {
    pub fn select_block(&mut self, block: &mut Block) {
        for inst in &mut block.insts {
            inst.flags &= !FLAG_X86_IMM_FORM;
            if is_immediate_candidate(inst) {
                inst.flags |= FLAG_X86_IMM_FORM;
                self.immediate_forms = self.immediate_forms.saturating_add(1);
            }
        }
    }

    #[inline]
    pub fn select(&self, inst: &IRInst) -> Selection {
        if inst.flags & FLAG_X86_IMM_FORM != 0 {
            Selection { opcode: inst.opcode, form: InstForm::Immediate, cost: 1 }
        } else {
            Selection { opcode: inst.opcode, form: InstForm::Generic, cost: generic_cost(inst.opcode) }
        }
    }
}

#[inline]
fn is_immediate_candidate(inst: &IRInst) -> bool {
    if !matches!(inst.opcode, Opcode::Add | Opcode::Sub | Opcode::And) {
        return false;
    }
    let width = match inst.a {
        Operand::Reg(reg) => reg.width,
        _ => return false,
    };
    let value = match inst.c {
        Operand::Imm(value) => value,
        _ => return false,
    };
    match width {
        // Operand-size 32 accepts the complete 32-bit immediate.
        RegWidth::W32 => value <= u32::MAX as u64,
        // 64-bit ADD/SUB/AND immediate encodings sign-extend imm32.
        RegWidth::X64 => (value as i32 as i64 as u64) == value,
    }
}

#[inline]
fn generic_cost(opcode: Opcode) -> u8 {
    match opcode {
        Opcode::Nop => 0,
        Opcode::Mov => 1,
        Opcode::Add | Opcode::Sub | Opcode::And | Opcode::Orr | Opcode::Eor => 2,
        Opcode::Shift => 2,
        Opcode::Load | Opcode::Store => 3,
        Opcode::Branch | Opcode::BranchCond | Opcode::Ret => 4,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{GuestReg, IRInst, Operand};

    #[test]
    fn selects_add_immediate() {
        let mut block = Block::at(0x1000);
        block.push(IRInst { opcode: Opcode::Add, flags: 0, a: Operand::Reg(GuestReg::x(0)), b: Operand::Reg(GuestReg::x(1)), c: Operand::Imm(7) });
        let mut selector = InstructionSelector::default();
        selector.select_block(&mut block);
        assert_ne!(block.insts[0].flags & FLAG_X86_IMM_FORM, 0);
        assert_eq!(selector.select(&block.insts[0]).form, InstForm::Immediate);
    }

    #[test]
    fn rejects_non_sign_extended_x64_immediate() {
        let mut block = Block::at(0x1000);
        block.push(IRInst { opcode: Opcode::Add, flags: 0, a: Operand::Reg(GuestReg::x(0)), b: Operand::Reg(GuestReg::x(1)), c: Operand::Imm(0x8000_0000) });
        let mut selector = InstructionSelector::default();
        selector.select_block(&mut block);
        assert_eq!(block.insts[0].flags & FLAG_X86_IMM_FORM, 0);
    }

    #[test]
    fn preserves_nzcv_bit() {
        let mut block = Block::at(0x1000);
        block.push(IRInst { opcode: Opcode::Sub, flags: crate::ir::FLAG_WRITES_NZCV, a: Operand::Reg(GuestReg::x(0)), b: Operand::Reg(GuestReg::x(1)), c: Operand::Imm(1) });
        let mut selector = InstructionSelector::default();
        selector.select_block(&mut block);
        assert_eq!(block.insts[0].flags & crate::ir::FLAG_WRITES_NZCV, crate::ir::FLAG_WRITES_NZCV);
        assert_ne!(block.insts[0].flags & FLAG_X86_IMM_FORM, 0);
    }
}
