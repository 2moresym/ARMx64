use armx64::codegen::CodeBuffer;
use armx64::ir::{Block, Opcode};
use armx64::runtime::{GuestMemory, GuestState};

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

fn run_with_memory(words: &[u32]) -> GuestState {
    let mut memory = GuestMemory::map(0x1000_0000).expect("fixed guest memory mapping failed");
    let mut block = Block::new();
    let consumed = armx64::lift::lift_block(words, &mut block);
    assert_eq!(consumed, words.len());
    assert!(block.insts.iter().all(|inst| inst.opcode != Opcode::Unsupported), "IR={:?}", block.insts);

    let mut code = CodeBuffer::new();
    code.emit_block(&block).unwrap_or_else(|e| panic!("tier-1 codegen failed: {e:?}; IR={:?}", block.insts));
    let executable = code.into_executable().expect("executable mapping failed");
    let mut state = GuestState::new();
    unsafe { (executable.entry())(&mut state) };
    assert_eq!(memory.read_u64(0x108), 50);
    let result = state.gpr[2];
    drop(memory);
    assert_eq!(result, 50);
    state
}

fn main() {
    let state = run(&[0xd2800540, 0x91002000, 0xd503201f]);
    println!("ARMx64 smoke: X0={} (expected 50)", state.gpr[0]);
    assert_eq!(state.gpr[0], 50);

    let state = run(&[0x529fffe0, 0x11000400, 0xd503201f]);
    println!("ARMx64 smoke: W32->X0={} (expected 65536)", state.gpr[0]);
    assert_eq!(state.gpr[0], 65536);

    let state = run(&[0x91001fe0, 0xd503201f]);
    println!("ARMx64 smoke: XZR read X0={} (expected 7)", state.gpr[0]);
    assert_eq!(state.gpr[0], 7);

    // mov x0,#50; mov x1,#0x100; str x0,[x1,#8]; ldr x2,[x1,#8]; nop
    let state = run_with_memory(&[
        0xd2800640,
        0xd2802001,
        0xf9000420,
        0xf9400422,
        0xd503201f,
    ]);
    println!("ARMx64 smoke: LDR/STR X2={} (expected 50)", state.gpr[2]);
    assert_eq!(state.gpr[2], 50);
}
