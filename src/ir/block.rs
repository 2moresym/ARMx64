use super::IRInst;

/// A single guest basic block represented in ARMx64 IR.
#[derive(Debug, Default)]
pub struct Block {
    pub insts: Vec<IRInst>,
    pub hit_count: u32,
}

impl Block {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn push(&mut self, inst: IRInst) {
        self.insts.push(inst);
    }
}
