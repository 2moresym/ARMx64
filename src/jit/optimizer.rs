use crate::ir::{Block, GuestReg, IRInst, Opcode, Operand, RegKind, RegWidth, ShiftKind, FLAG_WRITES_NZCV};

/// Small, architecture-aware IR optimizer used before machine-code emission.
///
/// This is intentionally conservative: it performs local constant propagation
/// and folding without changing memory ordering, NZCV semantics, or control flow.
#[derive(Debug, Default, Clone, Copy)]
pub struct Optimizer {
    pub folded: u32,
    pub propagated: u32,
    pub removed: u32,
}

impl Optimizer {
    pub fn optimize_block(&mut self, block: &mut Block) {
        let mut constants = [None::<u64>; 31];
        let mut out = Vec::with_capacity(block.insts.len());

        for mut inst in block.insts.drain(..) {
            self.rewrite_operand(&mut inst.b, &constants);
            self.rewrite_operand(&mut inst.c, &constants);

            match inst.opcode {
                Opcode::Nop => {
                    self.removed = self.removed.saturating_add(1);
                    continue;
                }
                Opcode::Mov => {
                    if let (Operand::Reg(dst), Operand::Imm(value)) = (inst.a, inst.b) {
                        if dst.kind == RegKind::General {
                            constants[dst.num as usize] = Some(mask_width(value, dst.width));
                        }
                    } else if let (Operand::Reg(dst), Operand::Reg(src)) = (inst.a, inst.b) {
                        if dst.kind == RegKind::General {
                            constants[dst.num as usize] = constants[src.num as usize].map(|v| mask_width(v, dst.width));
                        }
                    }
                }
                Opcode::Add | Opcode::Sub | Opcode::And | Opcode::Orr | Opcode::Eor => {
                    let dst = match inst.a { Operand::Reg(r) if r.kind == RegKind::General => r, _ => { out.push(inst); continue; } };
                    let lhs = constant_operand(inst.b, &constants);
                    let rhs = constant_operand(inst.c, &constants);
                    if let (Some(lhs), Some(rhs)) = (lhs, rhs) {
                        let value = fold_binop(inst.opcode, lhs, rhs, dst.width);
                        constants[dst.num as usize] = Some(value);
                        if inst.flags & FLAG_WRITES_NZCV == 0 {
                            inst.opcode = Opcode::Mov;
                            inst.b = Operand::Imm(value);
                            inst.c = Operand::None;
                            self.folded = self.folded.saturating_add(1);
                        }
                    } else {
                        constants[dst.num as usize] = None;
                    }
                }
                Opcode::Shift => {
                    let dst = match inst.a { Operand::Reg(r) if r.kind == RegKind::General => r, _ => { out.push(inst); continue; } };
                    if let Operand::ShiftReg { value, amount, kind } = inst.c {
                        if let (Some(v), Some(a)) = (constants[value.num as usize], constants[amount.num as usize]) {
                            let folded = fold_shift(v, a, kind, dst.width);
                            if inst.flags & FLAG_WRITES_NZCV == 0 {
                                inst.opcode = Opcode::Mov;
                                inst.b = Operand::Imm(folded);
                                inst.c = Operand::None;
                                self.folded = self.folded.saturating_add(1);
                            }
                            constants[dst.num as usize] = Some(folded);
                        } else {
                            constants[dst.num as usize] = None;
                        }
                    } else {
                        constants[dst.num as usize] = None;
                    }
                }
                Opcode::Load => {
                    invalidate_dest(inst.a, &mut constants);
                }
                Opcode::Unsupported => {
                    constants.fill(None);
                }
                _ => {}
            }

            if let Operand::Reg(dst) = inst.a {
                if dst.kind == RegKind::General && !matches!(inst.opcode, Opcode::Mov | Opcode::Add | Opcode::Sub | Opcode::And | Opcode::Orr | Opcode::Eor | Opcode::Shift) {
                    constants[dst.num as usize] = None;
                }
            }
            out.push(inst);
        }

        block.insts = out;
    }

    fn rewrite_operand(&mut self, op: &mut Operand, constants: &[Option<u64>; 31]) {
        let reg = match *op { Operand::Reg(r) if r.kind == RegKind::General => r, _ => return };
        if let Some(value) = constants[reg.num as usize] {
            *op = Operand::Imm(mask_width(value, reg.width));
            self.propagated = self.propagated.saturating_add(1);
        }
    }
}

fn constant_operand(op: Operand, constants: &[Option<u64>; 31]) -> Option<u64> {
    match op {
        Operand::Imm(v) => Some(v),
        Operand::Reg(r) if r.kind == RegKind::Zero => Some(0),
        Operand::Reg(r) if r.kind == RegKind::General => constants[r.num as usize].map(|v| mask_width(v, r.width)),
        _ => None,
    }
}

fn mask_width(value: u64, width: RegWidth) -> u64 {
    match width { RegWidth::W32 => value as u32 as u64, RegWidth::X64 => value }
}

fn fold_binop(op: Opcode, lhs: u64, rhs: u64, width: RegWidth) -> u64 {
    let value = match op { Opcode::Add => lhs.wrapping_add(rhs), Opcode::Sub => lhs.wrapping_sub(rhs), Opcode::And => lhs & rhs, Opcode::Orr => lhs | rhs, Opcode::Eor => lhs ^ rhs, _ => unreachable!() };
    mask_width(value, width)
}

fn fold_shift(value: u64, amount: u64, kind: ShiftKind, width: RegWidth) -> u64 {
    let bits = match width { RegWidth::W32 => 32, RegWidth::X64 => 64 };
    let value = mask_width(value, width);
    let shift = (amount as u32) & (bits - 1);
    let result = match kind {
        ShiftKind::Lsl => value.wrapping_shl(shift),
        ShiftKind::Lsr => value.wrapping_shr(shift),
        ShiftKind::Asr => match width { RegWidth::W32 => ((value as u32 as i32) >> shift) as u32 as u64, RegWidth::X64 => ((value as i64) >> shift) as u64 },
        ShiftKind::Ror => value.rotate_right(shift),
    };
    mask_width(result, width)
}

fn invalidate_dest(op: Operand, constants: &mut [Option<u64>; 31]) {
    if let Operand::Reg(GuestReg { num, kind: RegKind::General, .. }) = op { constants[num as usize] = None; }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::GuestReg;

    #[test]
    fn folds_constant_arithmetic() {
        let mut block = Block::at(0x1000);
        block.push(IRInst { opcode: Opcode::Mov, flags: 0, a: Operand::Reg(GuestReg::x(0)), b: Operand::Imm(40), c: Operand::None });
        block.push(IRInst { opcode: Opcode::Add, flags: 0, a: Operand::Reg(GuestReg::x(1)), b: Operand::Reg(GuestReg::x(0)), c: Operand::Imm(2) });
        let mut opt = Optimizer::default();
        opt.optimize_block(&mut block);
        assert_eq!(block.insts[1].opcode, Opcode::Mov);
        assert_eq!(block.insts[1].b, Operand::Imm(42));
        assert!(opt.folded >= 1);
    }

    #[test]
    fn preserves_flag_writers() {
        let mut block = Block::at(0x1000);
        block.push(IRInst { opcode: Opcode::Add, flags: FLAG_WRITES_NZCV, a: Operand::Reg(GuestReg::x(0)), b: Operand::Imm(1), c: Operand::Imm(2) });
        let mut opt = Optimizer::default();
        opt.optimize_block(&mut block);
        assert_eq!(block.insts[0].opcode, Opcode::Add);
        assert_eq!(block.insts[0].flags, FLAG_WRITES_NZCV);
    }

    #[test]
    fn folds_variable_shift_when_amount_is_constant() {
        let mut block = Block::at(0x1000);
        block.push(IRInst { opcode: Opcode::Mov, flags: 0, a: Operand::Reg(GuestReg::x(0)), b: Operand::Imm(1), c: Operand::None });
        block.push(IRInst { opcode: Opcode::Mov, flags: 0, a: Operand::Reg(GuestReg::x(1)), b: Operand::Imm(4), c: Operand::None });
        block.push(IRInst { opcode: Opcode::Shift, flags: 0, a: Operand::Reg(GuestReg::x(2)), b: Operand::Reg(GuestReg::x(0)), c: Operand::ShiftReg { value: GuestReg::x(0), amount: GuestReg::x(1), kind: ShiftKind::Lsl } });
        let mut opt = Optimizer::default();
        opt.optimize_block(&mut block);
        assert_eq!(block.insts[2].opcode, Opcode::Mov);
        assert_eq!(block.insts[2].b, Operand::Imm(16));
    }
}
