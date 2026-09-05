# ARMx64

High-performance AArch64 to x86-64 dynamic translation.

## Current architecture

- Single packed ARMx64 IR
- SSA-style `Value(u32)` identifiers
- Cold block code generation
- Hotness counters for promotion
- Lock-free code-cache pointer publication
- Fixed-offset guest memory abstraction
- x86-64-v2 target baseline

Trace compilation, lazy NZCV, register allocation, and direct machine-code emission are being built incrementally on top of this foundation.
