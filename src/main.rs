mod arch;
mod codegen;
mod ir;
mod jit;
mod lift;
mod runtime;

fn main() {
    println!("ARMx64 v{}", env!("CARGO_PKG_VERSION"));
    println!("AArch64 -> ARMx64 IR -> x86-64-v2");
}
