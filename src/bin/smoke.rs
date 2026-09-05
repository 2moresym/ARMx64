use armx64::codegen::CodeBuffer;
use armx64::ir::{Block, Opcode};
use armx64::runtime::GuestState;

fn run(words: &[u32]) -> GuestState {
    let mut block = Block::new();
    let consumed = armx64::lift::lift_block(words, &mut block);
    assert_eq!(consumed, words.len());
    assert!(block.insts.iter().all(|inst| inst.opcode != Opcode::Unsupported), "IR={:?}", block.insts);

    let mut code = CodeBuffer::new();
    code.emit_block(&block).unwrap_or_else(|e| panic!("tier-1 codegen failed: {e:?}; IR={:?}", block.insts));
    let executable = code.into_executable().expect("executable mapping failed");
    let mut state = GuestState::new();
    unsafe { (executable.entry())(&mut state) };
    state
}

fn main() {
    // 64-bit path: mov x0, #42; add x0, x0, #8; nop
    let state = run(&[0xd2800540, 0x91002000, 0xd503201f]);
    println!("ARMx64 smoke: X0={} (expected 50)", state.gpr[0]);
    assert_eq!(state.gpr[0], 50);

    // W32 path: mov w0, #0xfff; add w0, w0, #1.
    // A W-register write must leave the architectural X register zero-extended.
    let state = run(&[0x5281ffe0, 0x11000400, 0xd503201f]);
    println!("ARMx64 smoke: W32->X0={} (expected 4096)", state.gpr[0]);
    assert_eq!(state.gpr[0], 4096);

    // Reading XZR must produce zero: add x0, xzr, #7.
    let state = run(&[0x91001fe0, 0xd503201f]);
    println!("ARMx64 smoke: XZR read X0={} (expected 7)", state.gpr[0]);
    assert_eq!(state.gpr[0], 7);
}
