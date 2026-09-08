mod cache;
mod compiler;
mod hotness;
mod isel;
mod optimizer;

pub use cache::CodeCache;
pub use compiler::{BackgroundCompiler, CompileRequest, CompiledBlock};
pub use hotness::Hotness;
pub use isel::{InstructionSelector, InstForm, Selection};
pub use optimizer::Optimizer;

/// Run the target-independent optimizer followed by target-aware instruction
/// selection. Keeping these passes explicit makes the compiler usable for both
/// eager/AOT compilation and future hot-tier JIT compilation.
#[inline]
pub fn prepare_block(block: &mut crate::ir::Block) {
    let mut optimizer = Optimizer::default();
    optimizer.optimize_block(block);
    let mut selector = InstructionSelector::default();
    selector.select_block(block);
}
