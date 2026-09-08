use crate::ir::{Block, IRInst, Opcode, Operand, RegWidth};

/// Target-aware selection metadata. Semantic IR flags are never reused for
/// compiler hints: architectural NZCV state must only be written by the lifter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstForm { Generic, Immediate }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection { pub opcode: Opcode, pub form: InstForm, pub cost: u8 }

#[derive(Debug, Default, Clone, Copy)]
pub struct InstructionSelector { pub immediate_forms: u32 }

impl InstructionSelector {
    /// Analyze a block without mutating architectural flags.
    pub fn select_block(&mut self, block: &mut Block) {
        for inst in &block.insts {
            if is_immediate_candidate(inst) { self.immediate_forms = self.immediate_forms.saturating_add(1); }
        }
    }

    #[inline]
    pub fn select(&self, inst: &IRInst) -> Selection {
        if is_immediate_candidate(inst) { Selection { opcode: inst.opcode, form: InstForm::Immediate, cost: 1 } }
        else { Selection { opcode: inst.opcode, form: InstForm::Generic, cost: generic_cost(inst.opcode) } }
    }
}

#[inline]
fn is_immediate_candidate(inst: &IRInst) -> bool {
    if !matches!(inst.opcode, Opcode::Add | Opcode::Sub | Opcode::And | Opcode::Orr | Opcode::Eor) { return false; }
    let width = match inst.a { Operand::Reg(reg) => reg.width, _ => return false };
    let value = match inst.c { Operand::Imm(value) => value, _ => return false };
    match width { RegWidth::W32 => value <= u32::MAX as u64, RegWidth::X64 => (value as i32 as i64 as u64) == value }
}

#[inline]
fn generic_cost(opcode: Opcode) -> u8 {
    match opcode { Opcode::Nop => 0, Opcode::Mov => 1, Opcode::Add | Opcode::Sub | Opcode::And | Opcode::Orr | Opcode::Eor => 2, Opcode::Shift => 2, Opcode::Load | Opcode::Store => 3, Opcode::Branch | Opcode::BranchCond | Opcode::Ret => 4, _ => 4 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{GuestReg, IRInst, Operand, FLAG_WRITES_NZCV};

    fn select(opcode: Opcode, width: RegWidth, imm: u64, flags: u16) -> bool {
        let mut block = Block::at(0x1000);
        block.push(IRInst { opcode, flags, a: Operand::Reg(match width { RegWidth::W32 => GuestReg::w(0), RegWidth::X64 => GuestReg::x(0) }), b: Operand::Reg(GuestReg::x(1)), c: Operand::Imm(imm) });
        let mut selector = InstructionSelector::default();
        selector.select_block(&mut block);
        selector.select(&block.insts[0]).form == InstForm::Immediate
    }

    #[test] fn selects_add_immediate() { assert!(select(Opcode::Add, RegWidth::X64, 7, 0)); }
    #[test] fn selects_logical_immediates() { assert!(select(Opcode::Orr, RegWidth::W32, 0xff, 0)); assert!(select(Opcode::Eor, RegWidth::W32, 0xff, 0)); }
    #[test] fn rejects_non_sign_extended_x64_immediate() { assert!(!select(Opcode::Add, RegWidth::X64, 0x8000_0000, 0)); }
    #[test] fn preserves_nzcv_bit() {
        let mut block = Block::at(0x1000);
        block.push(IRInst { opcode: Opcode::Sub, flags: FLAG_WRITES_NZCV, a: Operand::Reg(GuestReg::x(0)), b: Operand::Reg(GuestReg::x(1)), c: Operand::Imm(1) });
        let before = block.insts[0].flags;
        let mut selector = InstructionSelector::default();
        selector.select_block(&mut block);
        assert_eq!(block.insts[0].flags, before);
    }
}
