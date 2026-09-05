use hashbrown::HashMap;

use crate::codegen::{CodeBuffer, ExecutableCode};
use crate::ir::{Block, Opcode};
use crate::lift::lift_block_at;
use crate::runtime::GuestState;

/// A block cache entry owns the executable mapping for its guest PC.
pub struct CompiledBlock {
    pub code: ExecutableCode,
    pub hit_count: u32,
}

/// Block-first JIT dispatcher. Native blocks return here after updating guest PC.
#[derive(Default)]
pub struct Dispatcher {
    blocks: HashMap<u64, CompiledBlock>,
    pub blocks_executed: u64,
}

impl Dispatcher {
    #[inline]
    pub fn new() -> Self { Self::default() }

    pub fn compile_block(&mut self, guest_pc: u64, words: &[u32]) -> Result<usize, String> {
        let mut block = Block::at(guest_pc);
        let consumed = lift_block_at(words, guest_pc, &mut block);
        if block.insts.iter().any(|inst| matches!(inst.opcode, Opcode::Unsupported)) {
            return Err(format!("unsupported instruction in block at {guest_pc:#x}"));
        }
        let mut buffer = CodeBuffer::new();
        buffer.emit_block(&block).map_err(|e| format!("codegen failed at {guest_pc:#x}: {e:?}"))?;
        let code = buffer.into_executable().map_err(|e| format!("executable mapping failed: {e}"))?;
        self.blocks.insert(guest_pc, CompiledBlock { code, hit_count: 0 });
        Ok(consumed)
    }

    #[inline]
    pub fn has_block(&self, guest_pc: u64) -> bool { self.blocks.contains_key(&guest_pc) }

    #[inline]
    pub fn block_count(&self) -> usize { self.blocks.len() }

    #[inline]
    pub fn hit_count(&self, guest_pc: u64) -> u32 { self.blocks.get(&guest_pc).map_or(0, |entry| entry.hit_count) }

    /// Run cached blocks until a block is missing or the execution budget is exhausted.
    pub fn run(&mut self, state: &mut GuestState, max_blocks: usize) -> Result<u64, String> {
        for _ in 0..max_blocks {
            let pc = state.pc;
            let entry = self.blocks.get_mut(&pc).ok_or_else(|| format!("no translated block at {pc:#x}"))?;
            self.blocks_executed = self.blocks_executed.wrapping_add(1);
            entry.hit_count = entry.hit_count.saturating_add(1);
            let entry_fn = entry.code.entry();
            unsafe { entry_fn(state); }
            if !self.blocks.contains_key(&state.pc) { return Ok(state.pc); }
        }
        Err("dispatcher execution budget exhausted".into())
    }
}
