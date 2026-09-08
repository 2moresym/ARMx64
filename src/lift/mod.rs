use crate::arch::aarch64::{self, A64Inst};
use crate::ir::{Block, GuestReg, IRInst, Opcode, Operand, RegKind, RegWidth, ShiftKind, FLAG_WRITES_NZCV};
use yaxpeax_arm::armv8::a64::{Opcode as A64Opcode, Operand as A64Operand, ShiftStyle, SizeCode};

pub use block::{lift_block, lift_block_at};
mod block;

#[inline]
fn lower_reg(size: SizeCode, num: u16, sp: bool) -> Operand {
    let num = num as u8;
    let width = match size { SizeCode::X => RegWidth::X64, SizeCode::W => RegWidth::W32 };
    let kind = if sp && num == 31 { RegKind::StackPointer } else if num == 31 { RegKind::Zero } else { RegKind::General };
    Operand::Reg(GuestReg { num, width, kind })
}

#[inline]
fn lower_shift(style: ShiftStyle) -> Option<ShiftKind> {
    match style {
        ShiftStyle::LSL => Some(ShiftKind::Lsl),
        ShiftStyle::LSR => Some(ShiftKind::Lsr),
        ShiftStyle::ASR => Some(ShiftKind::Asr),
        ShiftStyle::ROR => Some(ShiftKind::Ror),
        _ => None,
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
fn lower_unsigned_mem(inst: A64Inst, load: bool) -> Option<(Operand, Operand)> {
    let word = inst.0;
    let class = word & 0x3fc0_0000;
    let is_load = class == 0x3940_0000;
    let is_store = class == 0x3900_0000;
    if (load && !is_load) || (!load && !is_store) { return None; }
    let size = ((word >> 30) & 0x3) as u8;
    let width = match size { 2 => RegWidth::W32, 3 => RegWidth::X64, _ => return None };
    let rt = (word & 0x1f) as u8;
    let rn = ((word >> 5) & 0x1f) as u8;
    let imm12 = ((word >> 10) & 0xfff) as i32;
    let offset = imm12.checked_shl(size as u32)?;
    let base = if rn == 31 { GuestReg::xsp() } else { GuestReg::x(rn) };
    let mem = Operand::mem(base, offset, width);
    let reg = Operand::Reg(match width { RegWidth::W32 => GuestReg::w(rt), RegWidth::X64 => GuestReg::x(rt) });
    Some(if load { (reg, mem) } else { (mem, reg) })
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
fn sign_extend_19(value: u32) -> i64 {
    let value = ((value & 0x7ffff) as i64) << 2;
    (value << 43) >> 43
}

#[inline]
fn lift_conditional_branch(inst: A64Inst, guest_pc: u64, block: &mut Block) -> bool {
    let word = inst.0;
    if (word & 0xff00_0010) == 0x5400_0000 {
        let offset = sign_extend_19(word >> 5);
        let target = ((guest_pc as i64).wrapping_add(offset)) as u64;
        let cond = word & 0xf;
        block.push(IRInst { opcode: Opcode::BranchCond, flags: 0, a: Operand::GuestPc(target), b: Operand::Imm(cond as u64), c: Operand::None });
        return true;
    }

    let cb_class = word & 0x7f00_0000;
    if cb_class == 0x3400_0000 || cb_class == 0x3500_0000 {
        let width = if (word & (1 << 31)) != 0 { RegWidth::X64 } else { RegWidth::W32 };
        let rt = (word & 0x1f) as u8;
        let reg = Operand::Reg(GuestReg { num: rt, width, kind: if rt == 31 { RegKind::Zero } else { RegKind::General } });
        let offset = sign_extend_19(word >> 5);
        let target = ((guest_pc as i64).wrapping_add(offset)) as u64;
        let cbnz = cb_class == 0x3500_0000;
        block.push(IRInst { opcode: Opcode::BranchCond, flags: 0, a: Operand::GuestPc(target), b: reg, c: Operand::Imm(cbnz as u64) });
        return true;
    }
    false
}

#[inline]
fn lift_move_wide(inst: A64Inst, block: &mut Block) -> bool {
    let word = inst.0;
    let class = word & 0x7f80_0000;
    let movz = class == 0x5280_0000;
    let movn = class == 0x1280_0000;
    if !movz && !movn { return false; }

    let width = if (word & (1 << 31)) != 0 { RegWidth::X64 } else { RegWidth::W32 };
    let hw = ((word >> 21) & 0x3) as u8;
    if width == RegWidth::W32 && hw > 1 { unsupported(inst, block); return true; }
    let rd = (word & 0x1f) as u8;
    let imm16 = ((word >> 5) & 0xffff) as u64;
    let value = imm16 << (hw as u32 * 16);
    let value = if movn { !value } else { value };
    let value = if width == RegWidth::W32 { value as u32 as u64 } else { value };
    block.push(IRInst { opcode: Opcode::Mov, flags: 0, a: Operand::Reg(match width { RegWidth::W32 => GuestReg::w(rd), RegWidth::X64 => GuestReg::x(rd) }), b: Operand::Imm(value), c: Operand::None });
    true
}

#[inline]
pub fn lift_one(inst: A64Inst, block: &mut Block) { lift_one_at(inst, block.guest_pc + (block.insts.len() as u64 * 4), block) }

#[inline]
pub fn lift_one_at(inst: A64Inst, guest_pc: u64, block: &mut Block) {
    if lift_conditional_branch(inst, guest_pc, block) { return; }
    if lift_move_wide(inst, block) { return; }
    if let Some((a, b)) = lower_unsigned_mem(inst, true) { block.push(IRInst { opcode: Opcode::Load, flags: 0, a, b, c: Operand::None }); return; }
    if let Some((a, b)) = lower_unsigned_mem(inst, false) { block.push(IRInst { opcode: Opcode::Store, flags: 0, a, b, c: Operand::None }); return; }
    let decoded = match aarch64::decode(inst) { Ok(d) => d, Err(_) => { unsupported(inst, block); return; } };
    let variable_shift = match decoded.opcode { A64Opcode::LSLV => Some(ShiftKind::Lsl), A64Opcode::LSRV => Some(ShiftKind::Lsr), A64Opcode::ASRV => Some(ShiftKind::Asr), A64Opcode::RORV => Some(ShiftKind::Ror), _ => None };
    if let Some(kind) = variable_shift {
        if let Some((a, b, c)) = variable_shift_operands(&decoded, kind) { block.push(IRInst { opcode: Opcode::Shift, flags: 0, a, b, c }); } else { unsupported(inst, block); }
        return;
    }
    let (ir_opcode, flags) = match decoded.opcode {
        A64Opcode::HINT if inst.0 == 0xd503_201f => (Opcode::Nop, 0),
        A64Opcode::ADD => (Opcode::Add, 0), A64Opcode::ADDS => (Opcode::Add, FLAG_WRITES_NZCV),
        A64Opcode::SUB => (Opcode::Sub, 0), A64Opcode::SUBS => (Opcode::Sub, FLAG_WRITES_NZCV),
        A64Opcode::AND => (Opcode::And, 0), A64Opcode::ANDS => (Opcode::And, FLAG_WRITES_NZCV),
        A64Opcode::ORR => (Opcode::Orr, 0), A64Opcode::EOR => (Opcode::Eor, 0),
        A64Opcode::B => (Opcode::Branch, 0), A64Opcode::RET => (Opcode::Ret, 0),
        _ => { unsupported(inst, block); return; }
    };
    let mut a = lower_operand(decoded.operands[0]);
    if ir_opcode == Opcode::Branch {
        a = match a { Operand::PCRelative(offset) => Operand::GuestPc(((guest_pc as i64) + offset) as u64), _ => { unsupported(inst, block); return; } };
    }
    block.push(IRInst { opcode: ir_opcode, flags, a, b: lower_operand(decoded.operands[1]), c: lower_operand(decoded.operands[2]) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Opcode;

    #[test] fn sp_is_only_register_31() { let mut block = Block::new(); lift_one(A64Inst(0x91002000), &mut block); match block.insts[0].a { Operand::Reg(r) => assert_eq!(r.kind, RegKind::General), _ => panic!() } }
    #[test] fn nop_encoding_is_recognized() { let mut block = Block::new(); lift_one(A64Inst(0xd503_201f), &mut block); assert_eq!(block.insts[0].opcode, Opcode::Nop); }
    #[test] fn direct_branch_target_is_absolute_guest_pc() { let mut block = Block::at(0x1000); lift_one_at(A64Inst(0x14000000), 0x1000, &mut block); assert_eq!(block.insts[0].a, Operand::GuestPc(0x1000)); }
    #[test] fn conditional_branch_is_absolute_and_terminal() { let mut block = Block::at(0x1000); lift_one_at(A64Inst(0x54000041), 0x1000, &mut block); assert_eq!(block.insts[0].opcode, Opcode::BranchCond); assert_eq!(block.insts[0].a, Operand::GuestPc(0x1008)); assert_eq!(block.insts[0].b, Operand::Imm(1)); }
    #[test] fn conditional_branch_negative_displacement() { let mut block = Block::at(0x2000); lift_one_at(A64Inst(0x54ffffe1), 0x2000, &mut block); assert_eq!(block.insts[0].a, Operand::GuestPc(0x1ffc)); }
    #[test] fn cbz_lifts_register_and_target() { let mut block = Block::at(0x2000); lift_one_at(A64Inst(0xb4000040), 0x2000, &mut block); assert_eq!(block.insts[0].opcode, Opcode::BranchCond); assert_eq!(block.insts[0].a, Operand::GuestPc(0x2008)); assert_eq!(block.insts[0].b, Operand::Reg(GuestReg::x(0))); assert_eq!(block.insts[0].c, Operand::Imm(0)); }

    #[test]
    fn movz_honors_wide_immediate_shift() {
        let mut block = Block::new();
        // movz x0, #0x1234, lsl #32
        lift_one(A64Inst(0xd2c24680), &mut block);
        assert_eq!(block.insts[0].opcode, Opcode::Mov);
        assert_eq!(block.insts[0].b, Operand::Imm(0x1234_0000_0000));
    }

    #[test]
    fn movn_honors_width_and_shift() {
        let mut block = Block::new();
        // movn w0, #0, lsl #16 => 0xff00ffff
        lift_one(A64Inst(0x12a00000), &mut block);
        assert_eq!(block.insts[0].opcode, Opcode::Mov);
        assert_eq!(block.insts[0].b, Operand::Imm(0xff00_ffff));
    }
}
