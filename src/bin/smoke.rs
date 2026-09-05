use armx64::codegen::CodeBuffer;
use armx64::ir::{Block, Opcode};
use armx64::runtime::GuestState;

fn main() {
    // mov x0, #42; add x0, x0, #8; nop
    let words = [0xd2800540, 0x91002000, 0xd503201f];
    let mut block = Block::new();
    let consumed = armx64::lift::lift_block(&words, &mut block);
    println!("decoded/lifted {} instructions", consumed);
    for (i, inst) in block.insts.iter().enumerate() {
        println!("  {i}: {inst:?}");
    }
    assert_eq!(consumed, 3);
    assert_eq!(block.insts.len(), 3);
    assert!(block.insts.iter().all(|inst| inst.opcode != Opcode::Unsupported));

    let mut code = CodeBuffer::new();
    code.emit_block(&block).unwrap_or_else(|e| panic!("tier-1 codegen failed: {e:?}; IR={:?}", block.insts));
    println!("generated {} x86-64 bytes", code.bytes.len());

    let executable = code.into_executable().expect("executable mapping failed");
    let mut state = GuestState::new();
    unsafe { (executable.entry())(&mut state) };
    assert_eq!(state.gpr[0], 50);
    println!("ARMx64 smoke: translated block executed successfully, X0={}", state.gpr[0]);
}
