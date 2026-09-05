use super::Value;

/// Width of an AArch64 general-purpose register operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegWidth {
    W32,
    X64,
}

/// Special meaning of register number 31 in an AArch64 operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegKind {
    General,
    StackPointer,
    Zero,
}

/// Guest AArch64 general-purpose register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuestReg {
    pub num: u8,
    pub width: RegWidth,
    pub kind: RegKind,
}

impl GuestReg {
    #[inline]
    pub const fn x(num: u8) -> Self {
        Self { num, width: RegWidth::X64, kind: if num == 31 { RegKind::Zero } else { RegKind::General } }
    }

    #[inline]
    pub const fn w(num: u8) -> Self {
        Self { num, width: RegWidth::W32, kind: if num == 31 { RegKind::Zero } else { RegKind::General } }
    }

    #[inline]
    pub const fn xsp() -> Self {
        Self { num: 31, width: RegWidth::X64, kind: RegKind::StackPointer }
    }

    #[inline]
    pub const fn wsp() -> Self {
        Self { num: 31, width: RegWidth::W32, kind: RegKind::StackPointer }
    }
}

/// An operand carried by ARMx64 IR.
///
/// Guest registers remain explicit until SSA construction. This prevents the
/// lifter from confusing architectural register numbers with SSA `Value`s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operand {
    None,
    Value(Value),
    Reg(GuestReg),
    Imm(u64),
    PCRelative(i64),
    RawInst(u32),
    ShiftedReg { reg: GuestReg, amount: u8 },
}

impl Operand {
    #[inline]
    pub const fn none() -> Self { Self::None }

    #[inline]
    pub const fn value(value: Value) -> Self { Self::Value(value) }

    #[inline]
    pub const fn imm(value: u64) -> Self { Self::Imm(value) }

    #[inline]
    pub const fn raw_inst(word: u32) -> Self { Self::RawInst(word) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_31_is_zero_or_sp() {
        assert_eq!(GuestReg::x(31).kind, RegKind::Zero);
        assert_eq!(GuestReg::w(31).kind, RegKind::Zero);
        assert_eq!(GuestReg::xsp().kind, RegKind::StackPointer);
        assert_eq!(GuestReg::wsp().kind, RegKind::StackPointer);
    }

    #[test]
    fn widths_are_explicit() {
        assert_eq!(GuestReg::x(0).width, RegWidth::X64);
        assert_eq!(GuestReg::w(0).width, RegWidth::W32);
    }
}
