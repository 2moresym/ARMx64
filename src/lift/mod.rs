use crate::arch::aarch64::A64Inst;
use crate::ir::Block;

/// Lift one decoded guest instruction into ARMx64 IR.
///
/// The first implementation intentionally starts small; correctness comes
/// before trace formation and aggressive optimization.
#[inline]
pub fn lift_one(_inst: A64Inst, _block: &mut Block) {
    // TODO: AArch64 decoder/lifter.
}
