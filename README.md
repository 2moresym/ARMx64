# ARMx64

A high-performance AArch64 to x86-64 dynamic translation engine written in Rust.

## Architecture Overview

- **Single Packed IR**: Clean and efficient intermediate representation optimized for translation.
- **SSA-style Identifiers**: `Value(u32)` based static single assignment formulation.
- **Adaptive Compilation**: Hotness counters for trace detection and promotion.
- **Lock-free Caches**: Lock-free code-cache pointer publication for low-overhead dispatch.
- **Guest Memory Model**: Fixed-offset guest memory abstraction for fast address translation.
- **Modern Baseline**: Targeting x86-64-v2 architecture baseline.

## Running an AArch64 ELF

The runtime can now load an AArch64 ELF, map its `PT_LOAD` segments into guest memory, start at the ELF entry point, translate blocks on demand, and execute them through the native x86-64 backend.

```text
AArch64 ELF
    -> ELF loader
    -> guest memory
    -> block discovery
    -> ARMx64 IR
    -> x86-64 machine code
    -> dispatcher
```

For the first milestone, use a small statically linked AArch64 ELF whose entry point can execute using the currently implemented instruction subset:

```bash
cargo run --release -- ./test-aarch64 [x0]
```

`[x0]` is optional and initializes guest X0. Returning through `X30 == 0` terminates the minimal launcher.

This is **not yet a general Linux process launcher**. Dynamic linking, the AArch64 Linux syscall ABI, signals, threads, TLS, `argv`/`envp`, and broader ISA coverage are still required for normal Linux applications and games.

## Status & Roadmap

The current execution milestone is real ELF loading and on-demand basic-block translation. Next priorities are broader AArch64 control flow and arithmetic, Linux syscall/process support, dynamic linking, and then hot trace compilation, lazy NZCV evaluation, register allocation, and direct machine-code optimization.

## License

Licensed under the GNU General Public License v3.0 or later (`LICENSE` or https://www.gnu.org/licenses/gpl-3.0.html).
