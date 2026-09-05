use std::mem::offset_of;

/// Architectural state visible to translated ARM64 code.
/// The JIT ABI passes a pointer to this structure in SysV64 `rdi`.
#[repr(C)]
#[derive(Clone, Debug, Default)]
pub struct GuestState {
    pub gpr: [u64; 31],
    pub sp: u64,
    pub nzcv: u32,
    pub _pad: u32,
    pub pc: u64,
}

impl GuestState {
    #[inline]
    pub const fn new() -> Self { Self { gpr: [0; 31], sp: 0, nzcv: 0, _pad: 0, pc: 0 } }
    #[inline]
    pub fn read_x(&self, reg: u8) -> u64 { match reg { 0..=30 => self.gpr[reg as usize], 31 => 0, _ => unreachable!() } }
    #[inline]
    pub fn write_x(&mut self, reg: u8, value: u64) { if reg < 31 { self.gpr[reg as usize] = value; } }
    #[inline]
    pub fn read_sp(&self) -> u64 { self.sp }
    #[inline]
    pub fn write_sp(&mut self, value: u64) { self.sp = value; }
}

pub const GPR_BASE: i32 = offset_of!(GuestState, gpr) as i32;
pub const SP_OFFSET: i32 = offset_of!(GuestState, sp) as i32;
pub const PC_OFFSET: i32 = offset_of!(GuestState, pc) as i32;
