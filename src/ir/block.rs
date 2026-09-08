use super::IRInst;

/// A single guest basic block represented in ARMx64 IR.
#[derive(Debug)]
pub struct Block {
    pub guest_pc: u64,
    pub insts: Vec<IRInst>,
    pub hit_count: u32,
    /// Guest byte span represented by this block. This is independent of the
    /// number of optimized IR instructions so compiler passes cannot corrupt
    /// the architectural fall-through PC by deleting or folding IR.
    guest_byte_len: u64,
}

impl Default for Block {
    fn default() -> Self { Self { guest_pc: 0, insts: Vec::new(), hit_count: 0, guest_byte_len: 0 } }
}

impl Block {
    #[inline]
    pub fn new() -> Self { Self::default() }

    #[inline]
    pub fn at(guest_pc: u64) -> Self { Self { guest_pc, ..Self::default() } }

    #[inline]
    pub fn push(&mut self, inst: IRInst) {
        self.insts.push(inst);
        self.guest_byte_len = self.guest_byte_len.saturating_add(4);
    }

    #[inline]
    pub fn byte_len(&self) -> u64 { self.guest_byte_len }
}
