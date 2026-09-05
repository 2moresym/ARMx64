/// A decoded AArch64 instruction word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct A64Inst(pub u32);
