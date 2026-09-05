# ARMx64

A high-performance AArch64 to x86-64 dynamic translation engine written in Rust.

## Architecture Overview

- **Single Packed IR**: Clean and efficient intermediate representation optimized for translation.
- **SSA-style Identifiers**: `Value(u32)` based static single assignment formulation.
- **Adaptive Compilation**: Hotness counters for trace detection and promotion.
- **Lock-free Caches**: Lock-free code-cache pointer publication for low-overhead dispatch.
- **Guest Memory Model**: Fixed-offset guest memory abstraction for fast address translation.
- **Modern Baseline**: Targeting x86-64-v2 architecture baseline.

## Status & Roadmap

Trace compilation, lazy NZCV flag evaluation, register allocation, and direct machine-code emission are currently being built incrementally on top of this foundation.

## License

Licensed under the GNU General Public License v3.0 or later (`LICENSE` or https://www.gnu.org/licenses/gpl-3.0.html).
