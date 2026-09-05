use std::env;
use std::process::ExitCode;

use armx64::elf;
use armx64::runtime::{Dispatcher, GuestState};

const MAX_BLOCKS: usize = 1_000_000;
const WORDS_PER_FETCH: usize = 32;

fn main() -> ExitCode {
    if let Err(error) = try_main() {
        eprintln!("ARMx64 error: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn try_main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os();
    let program = args.next().unwrap_or_else(|| "armx64".into());
    let path = args.next().ok_or_else(|| format!("usage: {} <aarch64-elf> [x0]", program.to_string_lossy()))?;
    let x0 = args.next().map(|value| value.to_string_lossy().parse::<u64>()).transpose()?;

    let mut image = elf::load(&path)?;
    println!("ARMx64 v{}", env!("CARGO_PKG_VERSION"));
    println!("AArch64 ELF -> ARMx64 IR -> x86-64");
    println!("ELF entry: {:#x}", image.entry);
    println!("ELF PT_LOAD segments: {}", image.segments.len());
    println!("Guest memory: {} MiB @ {:#x}", image.memory.size() / (1024 * 1024), image.memory.base());

    let mut state = GuestState::new();
    state.pc = image.entry;
    state.write_x(30, 0); // RET to zero is our host-side termination sentinel.
    if let Some(value) = x0 { state.write_x(0, value); }
    state.sp = (image.memory.size() as u64).saturating_sub(16) & !0xf;

    let mut dispatcher = Dispatcher::new();
    for _ in 0..MAX_BLOCKS {
        if state.pc == 0 { break; }
        let pc = usize::try_from(state.pc).map_err(|_| "guest PC does not fit host usize")?;
        if pc >= image.memory.size() || pc.checked_add(4).is_none_or(|end| end > image.memory.size()) {
            return Err(format!("guest PC {:#x} is outside mapped memory", state.pc).into());
        }

        if !dispatcher.has_block(state.pc) {
            let mut words = [0u32; WORDS_PER_FETCH];
            for (index, word) in words.iter_mut().enumerate() {
                let address = pc.checked_add(index * 4).ok_or("instruction fetch overflow")?;
                if address + 4 > image.memory.size() { break; }
                *word = image.memory.read_u32(address);
            }
            let consumed = dispatcher.compile_block(state.pc, &words)
                .map_err(|error| format!("translation failed at {:#x}: {error}", state.pc))?;
            println!("[translate] {:#x} -> {} instructions", state.pc, consumed);
        }

        dispatcher.run(&mut state, 1)?;
    }

    if state.pc != 0 {
        return Err(format!("execution budget exhausted at guest PC {:#x}", state.pc).into());
    }

    println!("[exit] X0={} ({:#x})", state.read_x(0), state.read_x(0));
    println!("[stats] blocks={} executions={}", dispatcher.block_count(), dispatcher.blocks_executed);
    Ok(())
}
