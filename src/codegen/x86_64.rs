use crate::ir::{Block, GuestReg, Opcode, Operand, RegKind, RegWidth, ShiftKind};
use crate::runtime::{GuestState, GPR_BASE, SP_OFFSET};
use memmap2::{Mmap, MmapMut};

/// SysV64 ABI for translated blocks: `extern "C" fn(*mut GuestState)`.
pub type GuestFn = unsafe extern "C" fn(*mut GuestState);

#[derive(Debug)]
pub enum CodegenError {
    UnsupportedOpcode(Opcode),
    UnsupportedOperand,
    FlagWritingInstruction,
}

#[derive(Debug, Default)]
pub struct CodeBuffer {
    pub bytes: Vec<u8>,
}

impl CodeBuffer {
    #[inline] pub fn new() -> Self { Self::default() }
    #[inline] pub fn emit8(&mut self, byte: u8) { self.bytes.push(byte); }
    #[inline] fn emit32(&mut self, value: u32) { self.bytes.extend_from_slice(&value.to_le_bytes()); }
    #[inline] fn emit64(&mut self, value: u64) { self.bytes.extend_from_slice(&value.to_le_bytes()); }
    #[inline] fn rex(&mut self, w: bool) { if w { self.emit8(0x48); } }

    #[inline]
    fn load_state(&mut self, reg: X86Scratch, guest: GuestReg) {
        if guest.kind == RegKind::Zero {
            self.xor(reg, reg, guest.width);
            return;
        }
        let disp = if guest.kind == RegKind::StackPointer { SP_OFFSET } else { GPR_BASE + guest.num as i32 * 8 };
        self.mov_load(reg, disp, guest.width);
    }

    #[inline]
    fn store_state(&mut self, guest: GuestReg, reg: X86Scratch) {
        if guest.kind == RegKind::Zero { return; }
        let disp = if guest.kind == RegKind::StackPointer { SP_OFFSET } else { GPR_BASE + guest.num as i32 * 8 };
        self.mov_store(disp, reg, guest.width);
    }

    fn load_operand(&mut self, op: Operand, reg: X86Scratch, width: RegWidth) -> Result<(), CodegenError> {
        match op {
            Operand::Reg(g) => self.load_state(reg, g),
            Operand::Imm(value) => self.mov_imm(reg, value, width),
            Operand::ShiftedImm { value, amount } => self.mov_imm(reg, (value as u64) << amount, width),
            Operand::ShiftedReg { reg: guest, kind, amount } => {
                self.load_state(reg, guest);
                self.shift_imm(reg, kind, amount, width);
            }
            _ => return Err(CodegenError::UnsupportedOperand),
        }
        Ok(())
    }

    #[inline]
    fn mov_imm(&mut self, reg: X86Scratch, value: u64, width: RegWidth) {
        match (reg, width) {
            (X86Scratch::Rax, RegWidth::X64) => { self.emit8(0x48); self.emit8(0xB8); self.emit64(value); }
            (X86Scratch::Rcx, RegWidth::X64) => { self.emit8(0x48); self.emit8(0xB9); self.emit64(value); }
            (X86Scratch::Rax, RegWidth::W32) => { self.emit8(0xB8); self.emit32(value as u32); }
            (X86Scratch::Rcx, RegWidth::W32) => { self.emit8(0xB9); self.emit32(value as u32); }
        }
    }

    #[inline]
    fn mov_load(&mut self, reg: X86Scratch, disp: i32, width: RegWidth) {
        self.rex(width == RegWidth::X64);
        self.emit8(0x8B);
        self.emit8(match reg { X86Scratch::Rax => 0x87, X86Scratch::Rcx => 0x8F });
        self.emit32(disp as u32);
    }

    #[inline]
    fn mov_store(&mut self, disp: i32, reg: X86Scratch, width: RegWidth) {
        self.rex(width == RegWidth::X64);
        self.emit8(0x89);
        self.emit8(match reg { X86Scratch::Rax => 0x87, X86Scratch::Rcx => 0x8F });
        self.emit32(disp as u32);
    }

    #[inline]
    fn xor(&mut self, dst: X86Scratch, src: X86Scratch, width: RegWidth) {
        self.rex(width == RegWidth::X64);
        self.emit8(0x31);
        self.emit8(match (dst, src) {
            (X86Scratch::Rax, X86Scratch::Rax) => 0xC0,
            (X86Scratch::Rax, X86Scratch::Rcx) => 0xC8,
            (X86Scratch::Rcx, X86Scratch::Rax) => 0xC1,
            (X86Scratch::Rcx, X86Scratch::Rcx) => 0xC9,
        });
    }

    #[inline]
    fn binop(&mut self, op: Opcode, width: RegWidth) {
        self.rex(width == RegWidth::X64);
        self.emit8(match op { Opcode::Add => 0x01, Opcode::Sub => 0x29, Opcode::And => 0x21, Opcode::Orr => 0x09, Opcode::Eor => 0x31, _ => unreachable!() });
        self.emit8(0xC8); // rax <- rax OP rcx
    }

    #[inline]
    fn shift_imm(&mut self, reg: X86Scratch, kind: ShiftKind, amount: u8, width: RegWidth) {
        if amount == 0 { return; }
        self.rex(width == RegWidth::X64);
        self.emit8(0xC1);
        let rm = match (reg, kind) {
            (X86Scratch::Rax, ShiftKind::Lsl) => 0xE0, (X86Scratch::Rax, ShiftKind::Lsr) => 0xE8,
            (X86Scratch::Rax, ShiftKind::Asr) => 0xF8, (X86Scratch::Rax, ShiftKind::Ror) => 0xC8,
            (X86Scratch::Rcx, ShiftKind::Lsl) => 0xE1, (X86Scratch::Rcx, ShiftKind::Lsr) => 0xE9,
            (X86Scratch::Rcx, ShiftKind::Asr) => 0xF9, (X86Scratch::Rcx, ShiftKind::Ror) => 0xC9,
        };
        self.emit8(rm);
        self.emit8(amount);
    }

    #[inline]
    fn shift_reg(&mut self, kind: ShiftKind, width: RegWidth) {
        self.rex(width == RegWidth::X64);
        self.emit8(0xD3);
        self.emit8(match kind { ShiftKind::Lsl => 0xE0, ShiftKind::Lsr => 0xE8, ShiftKind::Asr => 0xF8, ShiftKind::Ror => 0xC8 });
    }

    pub fn emit_block(&mut self, block: &Block) -> Result<(), CodegenError> {
        for inst in &block.insts {
            if inst.flags != 0 { return Err(CodegenError::FlagWritingInstruction); }
            match inst.opcode {
                Opcode::Nop => self.emit8(0x90),
                Opcode::Mov => {
                    let dest = match inst.a { Operand::Reg(g) => g, _ => return Err(CodegenError::UnsupportedOperand) };
                    self.load_operand(inst.b, X86Scratch::Rax, dest.width)?;
                    self.store_state(dest, X86Scratch::Rax);
                }
                Opcode::Add | Opcode::Sub | Opcode::And | Opcode::Orr | Opcode::Eor => {
                    let dest = match inst.a { Operand::Reg(g) => g, _ => return Err(CodegenError::UnsupportedOperand) };
                    self.load_operand(inst.b, X86Scratch::Rax, dest.width)?;
                    self.load_operand(inst.c, X86Scratch::Rcx, dest.width)?;
                    self.binop(inst.opcode, dest.width);
                    self.store_state(dest, X86Scratch::Rax);
                }
                Opcode::Shift => {
                    let dest = match inst.a { Operand::Reg(g) => g, _ => return Err(CodegenError::UnsupportedOperand) };
                    self.load_operand(inst.b, X86Scratch::Rax, dest.width)?;
                    match inst.c {
                        Operand::ShiftReg { amount, kind, .. } => {
                            self.load_state(X86Scratch::Rcx, amount);
                            self.shift_reg(kind, dest.width);
                        }
                        Operand::ShiftedReg { kind, .. } => {
                            return Err(CodegenError::UnsupportedOperand).or_else(|_| { self.shift_imm(X86Scratch::Rax, kind, 0, dest.width); Err(CodegenError::UnsupportedOperand) });
                        }
                        _ => return Err(CodegenError::UnsupportedOperand),
                    }
                    self.store_state(dest, X86Scratch::Rax);
                }
                _ => return Err(CodegenError::UnsupportedOpcode(inst.opcode)),
            }
        }
        self.emit8(0xC3);
        Ok(())
    }

    pub fn into_executable(self) -> Result<ExecutableCode, std::io::Error> { ExecutableCode::from_bytes(&self.bytes) }
}

#[derive(Clone, Copy)]
enum X86Scratch { Rax, Rcx }

pub struct ExecutableCode {
    mapping: Mmap,
    entry: GuestFn,
}

impl ExecutableCode {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, std::io::Error> {
        let mut map = MmapMut::map_anon(bytes.len().max(1))?;
        map[..bytes.len()].copy_from_slice(bytes);
        let mapping = map.make_exec()?;
        let entry = unsafe { std::mem::transmute::<*const u8, GuestFn>(mapping.as_ptr()) };
        Ok(Self { mapping, entry })
    }

    #[inline] pub fn entry(&self) -> GuestFn { self.entry }
    #[inline] pub fn len(&self) -> usize { self.mapping.len() }
}
