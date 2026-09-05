mod block;

use crate::arch::aarch64::{self, A64Inst};
use crate::ir::{
    Block, GuestReg, IRInst, Opcode, Operand, RegKind, RegWidth, ShiftKind, FLAG_WRITES_NZCV,
};
use yaxpeax_arm::armv8::a64::{
    Opcode as A64Opcode, Operand as A64Operand, ShiftStyle, SizeCode,
};

pub use block::lift_block;

#[inline]
fn lower_reg(size: SizeCode, num: u16, allow_sp: bool) -> Operand {
    let num = num as u8;
    let width = match size { SizeCode::X => RegWidth::X64, SizeCode::W => RegWidth::W32 };
    let kind = if allow_sp && num == 31 {
        RegKind::StackPointer
    } else if num == 31 {
        RegKind::Zero
    } else {
        RegKind::General
    };
    Operand::Reg(GuestReg { num, width, kind })
}

#[inline]
fn lower_shift(style: ShiftStyle) -> Option<ShiftKind> {
    match style {
        ShiftStyle::LSL => Some(ShiftKind::Lsl), ShiftStyle::LSR => Some(ShiftKind::Lsr),
        ShiftStyle::ASR => Some(ShiftKind::Asr), ShiftStyle::ROR => Some(ShiftKind::Ror), _ => None,
    }
}

#[inline]
fn lower_operand(op: A64Operand) -> Operand {
    match op {
        A64Operand::Nothing => Operand::None,
        A64Operand::Register(size, num) => lower_reg(size, num, false),
        A64Operand::RegisterOrSP(size, num) => lower_reg(size, num, true),
        A64Operand::Immediate(value) => Operand::Imm(value as u64),
        A64Operand::Imm64(value) => Operand::Imm(value),
        A64Operand::Imm16(value) => Operand::Imm(value as u64),
        A64Operand::PCOffset(offset) => Operand::PCRelative(offset),
        A64Operand::ImmShift(value, amount) => Operand::ShiftedImm { value, amount },
        A64Operand::RegShift(style, amount, size, num) => match lower_shift(style) {
            Some(kind) => match lower_reg(size, num, false) {
                Operand::Reg(reg) => Operand::ShiftedReg { reg, kind, amount },
                _ => Operand::None,
            },
            None => Operand::None,
        },
        _ => Operand::None,
    }
}

#[inline]
fn unsupported(inst: A64Inst, block: &mut Block) {
    block.push(IRInst { opcode: Opcode::Unsupported, flags: 0, a: Operand::raw_inst(inst.0), b: Operand::None, c: Operand::None });
}

#[inline]
fn variable_shift_operands(decoded: &yaxpeax_arm::armv8::a64::Instruction, kind: ShiftKind) -> Option<(Operand, Operand, Operand)> {
    let dest = match lower_operand(decoded.operands[0]) { Operand::Reg(r) => Operand::Reg(r), _ => return None };
    let value = match lower_operand(decoded.operands[1]) { Operand::Reg(r) => r, _ => return None };
    let amount = match lower_operand(decoded.operands[2]) { Operand::Reg(r) => r, _ => return None };
    Some((dest, Operand::Reg(value), Operand::ShiftReg { value, amount, kind }))
}

#[inline]
pub fn lift_one(inst: A64Inst, block: &mut Block) {
    let decoded = match aarch64::decode(inst) {
        Ok(d) => d,
        Err(_) => { unsupported(inst, block); return; }
    };

    let variable_shift = match decoded.opcode {
        A64Opcode::LSLV => Some(ShiftKind::Lsl), A64Opcode::LSRV => Some(ShiftKind::Lsr),
        A64Opcode::ASRV => Some(ShiftKind::Asr), A64Opcode::RORV => Some(ShiftKind::Ror), _ => None,
    };
    if let Some(kind) = variable_shift {
        if let Some((a, b, c)) = variable_shift_operands(&decoded, kind) {
            block.push(IRInst { opcode: Opcode::Shift, flags: 0, a, b, c });
        } else { unsupported(inst, block); }
        return;
    }

    let (ir_opcode, flags) = match decoded.opcode {
        A64Opcode::HINT if inst.0 == 0xd503201f => (Opcode::Nop, 0),
        A64Opcode::ADD => (Opcode::Add, 0), A64Opcode::ADDS => (Opcode::Add, FLAG_WRITES_NZCV),
        A64Opcode::SUB => (Opcode::Sub, 0), A64Opcode::SUBS => (Opcode::Sub, FLAG_WRITES_NZCV),
        A64Opcode::AND => (Opcode::And, 0), A64Opcode::ANDS => (Opcode::And, FLAG_WRITES_NZCV),
        A64Opcode::ORR => (Opcode::Orr, 0), A64Opcode::EOR => (Opcode::Eor, 0),
        A64Opcode::MOVZ => (Opcode::Mov, 0), A64Opcode::B | A64Opcode::BR => (Opcode::Branch, 0),
        A64Opcode::Bcc(_) | A64Opcode::CBZ | A64Opcode::CBNZ => (Opcode::BranchCond, 0),
        A64Opcode::BL | A64Opcode::BLR => (Opcode::Call, 0), A64Opcode::RET => (Opcode::Ret, 0),
        _ => { unsupported(inst, block); return; }
    };

    block.push(IRInst { opcode: ir_opcode, flags, a: lower_operand(decoded.operands[0]), b: lower_operand(decoded.operands[1]), c: lower_operand(decoded.operands[2]) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lift_nop() {
        let mut block = Block::new(); lift_one(A64Inst(0xd503201f), &mut block);
        assert_eq!(block.insts.len(), 1); assert_eq!(block.insts[0].opcode, Opcode::Nop);
    }

    #[test]
    fn test_register_or_sp_only_maps_31_to_sp() {
        assert_eq!(lower_operand(A64Operand::RegisterOrSP(SizeCode::X, 0)), Operand::Reg(GuestReg::x(0)));
        assert_eq!(lower_operand(A64Operand::RegisterOrSP(SizeCode::X, 31)), Operand::Reg(GuestReg::xsp()));
    }

    #[test]
    fn test_unsupported_is_not_nop() {
        let mut block = Block::new(); lift_one(A64Inst(0xd61f0000), &mut block);
        assert_eq!(block.insts.len(), 1); assert_eq!(block.insts[0].opcode, Opcode::Unsupported);
        assert_eq!(block.insts[0].a, Operand::RawInst(0xd61f0000));
    }

    #[test]
    fn test_flag_setting_opcode_is_preserved() {
        let mut block = Block::new(); lift_one(A64Inst(0xab000000), &mut block);
        assert_eq!(block.insts.len(), 1); assert_eq!(block.insts[0].opcode, Opcode::Add);
        assert_ne!(block.insts[0].flags & FLAG_WRITES_NZCV, 0);
    }
}
