use armx64::codegen::CodeBuffer;
use armx64::ir::Block;
use armx64::runtime::GuestState;

fn main() {
    let words = [0xd2800540, 0x91002000, 0xd503201f]; // mov x0,#42; add x0,x0,#8; nop
    let mut block = Block::new();
    let consumed = armx64::lift::lift_block(&words, &mut block);
    assert_eq!(consumed, 3);

    let mut code = CodeBuffer::new();
    code.emit_block(&block).expect("tier-1 codegen failed");
    let executable = code.into_executable().expect("executable mapping failed");

    let mut state = GuestState::new();
    unsafe { (executable.entry())(&mut state) };
    assert_eq!(state.gpr[0], 50);
    println!("ARMx64 smoke: translated block executed successfully, X0={}", state.gpr[0]);
}
