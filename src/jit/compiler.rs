use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use crate::codegen::{CodeBuffer, ExecutableCode};
use crate::ir::Block;

pub struct CompileRequest { pub pc: u64, pub block: Block }
pub struct CompiledBlock { pub pc: u64, pub code: ExecutableCode }

/// Background tier-1 compiler. The guest execution thread only enqueues work.
pub struct BackgroundCompiler {
    tx: Option<Sender<CompileRequest>>,
    join: Option<JoinHandle<()>>,
}

impl BackgroundCompiler {
    pub fn spawn() -> (Self, Receiver<CompiledBlock>) {
        let (tx, rx) = mpsc::channel::<CompileRequest>();
        let (result_tx, result_rx) = mpsc::channel::<CompiledBlock>();
        let join = thread::Builder::new().name("armx64-compiler".into()).spawn(move || {
            while let Ok(request) = rx.recv() {
                let mut buffer = CodeBuffer::new();
                let Ok(()) = buffer.emit_block(&request.block) else { continue };
                let Ok(code) = buffer.into_executable() else { continue };
                if result_tx.send(CompiledBlock { pc: request.pc, code }).is_err() { break; }
            }
        }).expect("failed to start ARMx64 compiler thread");
        (Self { tx: Some(tx), join: Some(join) }, result_rx)
    }

    #[inline]
    pub fn enqueue(&self, request: CompileRequest) -> Result<(), mpsc::SendError<CompileRequest>> {
        self.tx.as_ref().expect("compiler is shutting down").send(request)
    }
}

impl Drop for BackgroundCompiler {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(join) = self.join.take() { let _ = join.join(); }
    }
}
