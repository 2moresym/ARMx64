use super::Value;

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

/// Fixed-width IR instruction. Kept compact for cache-friendly passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct IRInst {
    pub opcode: Opcode,
    pub flags: u16,
    pub a: Value,
    pub b: Value,
    pub c: Value,
}
