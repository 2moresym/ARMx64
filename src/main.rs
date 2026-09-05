fn main() {
    println!("ARMx64 v{}", env!("CARGO_PKG_VERSION"));
    println!("AArch64 -> ARMx64 IR -> x86-64-v2");
    println!("Use `cargo run --release --bin smoke` to execute the native translation smoke test.");
}
