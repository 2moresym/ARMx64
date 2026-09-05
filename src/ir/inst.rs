use super::Operand;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Opcode {
    Nop,
    Unsupported,
    Mov,
    Add,
    Sub,
    And,
    Orr,
    Eor,
    Shift,
    Extend,
    Load,
    Store,
    Compare,
    Branch,
    BranchCond,
    Call,
    Ret,
}

/// `flags` bit indicating that an arithmetic/logical instruction writes NZCV.
pub const FLAG_WRITES_NZCV: u16 = 1 << 0;

/// Fixed-width IR instruction with typed guest operands.
///
/// Operands are architectural until SSA construction; a `Value` is therefore
/// explicit as `Operand::Value` rather than being conflated with a register or
/// raw instruction word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct IRInst {
    pub opcode: Opcode,
    pub flags: u16,
    pub a: Operand,
    pub b: Operand,
    pub c: Operand,
}
