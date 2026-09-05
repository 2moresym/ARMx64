use super::IRInst;

/// A single guest basic block represented in ARMx64 IR.
#[derive(Debug)]
pub struct Block {
    pub guest_pc: u64,
    pub insts: Vec<IRInst>,
    pub hit_count: u32,
}

impl Default for Block {
    fn default() -> Self { Self { guest_pc: 0, insts: Vec::new(), hit_count: 0 } }
}

impl Block {
    #[inline]
    pub fn new() -> Self { Self::default() }

    #[inline]
    pub fn at(guest_pc: u64) -> Self { Self { guest_pc, ..Self::default() } }

    #[inline]
    pub fn push(&mut self, inst: IRInst) { self.insts.push(inst); }

    #[inline]
    pub fn byte_len(&self) -> u64 { (self.insts.len() as u64) * 4 }
}
