use armx64::codegen::CodeBuffer;
use armx64::ir::{Block, Opcode};
use armx64::runtime::{Dispatcher, GuestMemory, GuestState};

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
    let memory = GuestMemory::map(0x1000_0000).expect("fixed guest memory mapping failed");
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

    let state = run_with_memory(&[
        0xd2800640, 0xd2802001, 0xf9000420, 0xf9400422, 0xd503201f,
    ]);
    println!("ARMx64 smoke: LDR/STR X2={} (expected 50)", state.gpr[2]);
    assert_eq!(state.gpr[2], 50);

    // Block 0x1000: mov x0,#40; b 0x100c.
    // Block 0x100c: add x0,x0,#2; ret.
    let mut dispatcher = Dispatcher::new();
    let first = [0xd2800500, 0x14000002];
    let second = [0x91000800, 0xd65f03c0];
    assert_eq!(dispatcher.compile_block(0x1000, &first).unwrap(), 2);
    assert_eq!(dispatcher.compile_block(0x100c, &second).unwrap(), 2);
    let mut state = GuestState::new();
    state.pc = 0x1000;
    state.gpr[30] = 0;
    let final_pc = dispatcher.run(&mut state, 8).unwrap();
    println!("ARMx64 smoke: B/RET X0={} PC={:#x} blocks={}", state.gpr[0], final_pc, dispatcher.blocks_executed);
    assert_eq!(state.gpr[0], 42);
    assert_eq!(final_pc, 0);
}
