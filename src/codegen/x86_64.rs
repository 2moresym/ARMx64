use crate::ir::{Block, GuestReg, Opcode, Operand, RegKind, RegWidth, ShiftKind};
use crate::runtime::{GuestState, GPR_BASE, HOST_BASE, NZCV_OFFSET, PC_OFFSET, SP_OFFSET};
use memmap2::{Mmap, MmapMut};

pub type GuestFn = unsafe extern "C" fn(*mut GuestState);

#[derive(Debug)]
pub enum CodegenError { UnsupportedOpcode(Opcode), UnsupportedOperand }

#[derive(Debug, Default)]
pub struct CodeBuffer { pub bytes: Vec<u8> }

impl CodeBuffer {
    #[inline] pub fn new() -> Self { Self::default() }
    #[inline] pub fn emit8(&mut self, byte: u8) { self.bytes.push(byte); }
    #[inline] fn emit32(&mut self, value: u32) { self.bytes.extend_from_slice(&value.to_le_bytes()); }
    #[inline] fn emit64(&mut self, value: u64) { self.bytes.extend_from_slice(&value.to_le_bytes()); }
    #[inline] fn rex(&mut self, w: bool) { if w { self.emit8(0x48); } }

    #[inline]
    fn load_state(&mut self, reg: X86Scratch, guest: GuestReg) {
        if guest.kind == RegKind::Zero { self.xor(reg, reg, guest.width); return; }
        let disp = if guest.kind == RegKind::StackPointer { SP_OFFSET } else { GPR_BASE + guest.num as i32 * 8 };
        self.mov_load(reg, disp, guest.width);
    }

    #[inline]
    fn store_state(&mut self, guest: GuestReg, reg: X86Scratch) {
        if guest.kind == RegKind::Zero { return; }
        let disp = if guest.kind == RegKind::StackPointer { SP_OFFSET } else { GPR_BASE + guest.num as i32 * 8 };
        self.mov_store(disp, reg, RegWidth::X64);
    }

    fn load_operand(&mut self, op: Operand, reg: X86Scratch, width: RegWidth) -> Result<(), CodegenError> {
        match op {
            Operand::Reg(g) => self.load_state(reg, g),
            Operand::Imm(value) => self.mov_imm(reg, value, width),
            Operand::ShiftedImm { value, amount } => self.mov_imm(reg, (value as u64) << amount, width),
            Operand::ShiftedReg { reg: guest, kind, amount } => { self.load_state(reg, guest); self.shift_imm(reg, kind, amount, width); }
            _ => return Err(CodegenError::UnsupportedOperand),
        }
        Ok(())
    }

    #[inline]
    fn mov_imm(&mut self, reg: X86Scratch, value: u64, width: RegWidth) {
        match (reg, width) {
            (X86Scratch::Rax, RegWidth::X64) => { self.emit8(0x48); self.emit8(0xB8); self.emit64(value); }
            (X86Scratch::Rcx, RegWidth::X64) => { self.emit8(0x48); self.emit8(0xB9); self.emit64(value); }
            (X86Scratch::Rdx, RegWidth::X64) => { self.emit8(0x48); self.emit8(0xBA); self.emit64(value); }
            (X86Scratch::Rax, RegWidth::W32) => { self.emit8(0xB8); self.emit32(value as u32); }
            (X86Scratch::Rcx, RegWidth::W32) => { self.emit8(0xB9); self.emit32(value as u32); }
            (X86Scratch::Rdx, RegWidth::W32) => { self.emit8(0xBA); self.emit32(value as u32); }
        }
    }

    #[inline]
    fn mov_load(&mut self, reg: X86Scratch, disp: i32, width: RegWidth) {
        self.rex(width == RegWidth::X64); self.emit8(0x8B);
        self.emit8(match reg { X86Scratch::Rax => 0x87, X86Scratch::Rcx => 0x8F, X86Scratch::Rdx => 0x97 }); self.emit32(disp as u32);
    }

    #[inline]
    fn mov_store(&mut self, disp: i32, reg: X86Scratch, width: RegWidth) {
        self.rex(width == RegWidth::X64); self.emit8(0x89);
        self.emit8(match reg { X86Scratch::Rax => 0x87, X86Scratch::Rcx => 0x8F, X86Scratch::Rdx => 0x97 }); self.emit32(disp as u32);
    }

    #[inline]
    fn xor(&mut self, dst: X86Scratch, src: X86Scratch, width: RegWidth) {
        self.rex(width == RegWidth::X64); self.emit8(0x31);
        self.emit8(match (dst, src) { (X86Scratch::Rax, X86Scratch::Rax) => 0xC0, (X86Scratch::Rax, X86Scratch::Rcx) => 0xC8, (X86Scratch::Rcx, X86Scratch::Rax) => 0xC1, (X86Scratch::Rcx, X86Scratch::Rcx) => 0xC9, (X86Scratch::Rdx, X86Scratch::Rdx) => 0xD2, _ => unreachable!() });
    }

    #[inline]
    fn binop(&mut self, op: Opcode, width: RegWidth) {
        self.rex(width == RegWidth::X64); self.emit8(match op { Opcode::Add => 0x01, Opcode::Sub => 0x29, Opcode::And | Opcode::Orr => 0x21, Opcode::Eor => 0x31, _ => unreachable!() });
        if matches!(op, Opcode::Orr) { self.emit8(0xC8); return; }
        self.emit8(0xC8);
    }

    #[inline]
    fn emit_nzcv_from_x86_flags(&mut self) {
        self.xor(X86Scratch::Rdx, X86Scratch::Rdx, RegWidth::W32);
        // Build N:Z:C:V in bits 31..28 without disturbing the arithmetic flags.
        self.emit8(0x0F); self.emit8(0x99); self.emit8(0xC2); // setns dl
        self.emit8(0xC1); self.emit8(0xE2); self.emit8(0x01); // shl edx, 1
        self.emit8(0x0F); self.emit8(0x94); self.emit8(0xC2); // sete dl
        self.emit8(0xC1); self.emit8(0xE2); self.emit8(0x01);
        self.emit8(0x0F); self.emit8(0x92); self.emit8(0xC2); // setb dl
        self.emit8(0xC1); self.emit8(0xE2); self.emit8(0x01);
        self.emit8(0x0F); self.emit8(0x90); self.emit8(0xC2); // seto dl
        self.emit8(0xC1); self.emit8(0xE2); self.emit8(0x1C); // shl edx, 28
        self.mov_store(NZCV_OFFSET, X86Scratch::Rdx, RegWidth::W32);
    }

    #[inline]
    fn shift_imm(&mut self, reg: X86Scratch, kind: ShiftKind, amount: u8, width: RegWidth) {
        if amount == 0 { return; }
        self.rex(width == RegWidth::X64); self.emit8(0xC1);
        let rm = match (reg, kind) { (X86Scratch::Rax, ShiftKind::Lsl) => 0xE0, (X86Scratch::Rax, ShiftKind::Lsr) => 0xE8, (X86Scratch::Rax, ShiftKind::Asr) => 0xF8, (X86Scratch::Rax, ShiftKind::Ror) => 0xC8, (X86Scratch::Rcx, ShiftKind::Lsl) => 0xE1, (X86Scratch::Rcx, ShiftKind::Lsr) => 0xE9, (X86Scratch::Rcx, ShiftKind::Asr) => 0xF9, (X86Scratch::Rcx, ShiftKind::Ror) => 0xC9, (X86Scratch::Rdx, ShiftKind::Lsl) => 0xE2, (X86Scratch::Rdx, ShiftKind::Lsr) => 0xEA, (X86Scratch::Rdx, ShiftKind::Asr) => 0xFA, (X86Scratch::Rdx, ShiftKind::Ror) => 0xCA };
        self.emit8(rm); self.emit8(amount);
    }

    #[inline]
    fn shift_reg(&mut self, kind: ShiftKind, width: RegWidth) {
        self.rex(width == RegWidth::X64); self.emit8(0xD3); self.emit8(match kind { ShiftKind::Lsl => 0xE0, ShiftKind::Lsr => 0xE8, ShiftKind::Asr => 0xF8, ShiftKind::Ror => 0xC8 });
    }

    #[inline]
    fn emit_guest_address(&mut self, base: GuestReg) {
        self.load_state(X86Scratch::Rax, base);
        self.mov_imm(X86Scratch::Rdx, HOST_BASE as u64, RegWidth::X64);
        self.rex(true); self.emit8(0x01); self.emit8(0xD0);
    }

    #[inline]
    fn emit_mem_load(&mut self, width: RegWidth, offset: i32) {
        self.rex(width == RegWidth::X64); self.emit8(0x8B); self.emit8(0x80); self.emit32(offset as u32);
    }

    #[inline]
    fn emit_mem_store(&mut self, width: RegWidth, offset: i32) {
        self.rex(width == RegWidth::X64); self.emit8(0x89); self.emit8(0x88); self.emit32(offset as u32);
    }

    #[inline]
    fn emit_set_pc_imm(&mut self, pc: u64) {
        self.mov_imm(X86Scratch::Rax, pc, RegWidth::X64);
        self.mov_store(PC_OFFSET, X86Scratch::Rax, RegWidth::X64);
    }

    #[inline]
    fn emit_branch_cond(&mut self, target: u64, condition: Operand, fallthrough: u64) -> Result<(), CodegenError> {
        match condition {
            // B.cond: reduce NZCV to a 4-bit index and test a 16-bit truth table.
            Operand::Imm(cond) => {
                if cond > 15 { return Err(CodegenError::UnsupportedOperand); }
                let tables: [u16; 16] = [0xF0F0, 0xCCCC, 0xFCFC, 0xFF00, 0xAAAA, 0x3333, 0x0F0F, 0x0303, 0xAA55, 0x55AA, 0x0A05, 0xF5FA, 0xFFFF, 0x0000, 0x0C0C, 0xF3F3];
                self.mov_load(X86Scratch::Rax, NZCV_OFFSET, RegWidth::W32);
                self.emit8(0xC1); self.emit8(0xE8); self.emit8(0x1C); // shr eax, 28
                self.mov_imm(X86Scratch::Rcx, tables[cond as usize] as u64, RegWidth::W32);
                self.emit8(0x66); self.emit8(0x0F); self.emit8(0xA3); self.emit8(0xC1); // bt cx, ax
                self.emit8(0x73); self.emit8(0x12); // jnc skip taken (18 bytes)
            }
            // CBZ/CBNZ: c==0 => CBZ, c==1 => CBNZ.
            Operand::Reg(reg) => {
                let cbnz = match condition { _ => return Err(CodegenError::UnsupportedOperand) };
                let _ = cbnz;
                self.load_state(X86Scratch::Rax, reg);
                self.rex(reg.width == RegWidth::X64); self.emit8(0x85); self.emit8(0xC0); // test rax/eax, itself
                // This form is only selected after the caller validates c.
            }
            _ => return Err(CodegenError::UnsupportedOperand),
        }
        self.emit_set_pc_imm(target); self.emit8(0xC3);
        self.emit_set_pc_imm(fallthrough); self.emit8(0xC3);
        Ok(())
    }

    pub fn emit_block(&mut self, block: &Block) -> Result<(), CodegenError> {
        for (index, inst) in block.insts.iter().enumerate() {
            let is_last = index + 1 == block.insts.len();
            match inst.opcode {
                Opcode::Nop => self.emit8(0x90),
                Opcode::Mov => {
                    let dest = match inst.a { Operand::Reg(g) => g, _ => return Err(CodegenError::UnsupportedOperand) };
                    self.load_operand(inst.b, X86Scratch::Rax, dest.width)?; self.store_state(dest, X86Scratch::Rax);
                }
                Opcode::Add | Opcode::Sub | Opcode::And | Opcode::Orr | Opcode::Eor => {
                    let dest = match inst.a { Operand::Reg(g) => g, _ => return Err(CodegenError::UnsupportedOperand) };
                    self.load_operand(inst.b, X86Scratch::Rax, dest.width)?; self.load_operand(inst.c, X86Scratch::Rcx, dest.width); self.load_operand(inst.c, X86Scratch::Rcx, dest.width)?;
                    self.binop(inst.opcode, dest.width);
                    if inst.flags != 0 { self.emit_nzcv_from_x86_flags(); }
                    self.store_state(dest, X86Scratch::Rax);
                }
                Opcode::Shift => {
                    let dest = match inst.a { Operand::Reg(g) => g, _ => return Err(CodegenError::UnsupportedOperand) };
                    self.load_operand(inst.b, X86Scratch::Rax, dest.width)?;
                    match inst.c { Operand::ShiftReg { amount, kind, .. } => { self.load_state(X86Scratch::Rcx, amount); self.shift_reg(kind, dest.width); }, _ => return Err(CodegenError::UnsupportedOperand) }
                    self.store_state(dest, X86Scratch::Rax);
                }
                Opcode::Load => {
                    let dest = match inst.a { Operand::Reg(g) => g, _ => return Err(CodegenError::UnsupportedOperand) };
                    let mem = match inst.b { Operand::Mem(m) => m, _ => return Err(CodegenError::UnsupportedOperand) };
                    self.emit_guest_address(mem.base); self.emit_mem_load(mem.width, mem.offset); self.store_state(dest, X86Scratch::Rax);
                }
                Opcode::Store => {
                    let mem = match inst.a { Operand::Mem(m) => m, _ => return Err(CodegenError::UnsupportedOperand) };
                    let src = match inst.b { Operand::Reg(g) => g, _ => return Err(CodegenError::UnsupportedOperand) };
                    self.load_state(X86Scratch::Rcx, src); self.emit_guest_address(mem.base); self.emit_mem_store(mem.width, mem.offset);
                }
                Opcode::Branch => match inst.a { Operand::GuestPc(pc) => { self.emit_set_pc_imm(pc); self.emit8(0xC3); return Ok(()); }, _ => return Err(CodegenError::UnsupportedOperand) },
                Opcode::BranchCond => {
                    let target = match inst.a { Operand::GuestPc(pc) => pc, _ => return Err(CodegenError::UnsupportedOperand) };
                    let fallthrough = block.guest_pc + (index as u64 + 1) * 4;
                    match (inst.b, inst.c) {
                        (Operand::Imm(cond), Operand::None) => self.emit_branch_cond(target, Operand::Imm(cond), fallthrough)?,
                        (Operand::Reg(reg), Operand::Imm(kind)) if kind <= 1 => {
                            self.load_state(X86Scratch::Rax, reg);
                            self.rex(reg.width == RegWidth::X64); self.emit8(0x85); self.emit8(0xC0);
                            self.emit8(if kind == 0 { 0x75 } else { 0x74 }); self.emit8(0x12);
                            self.emit_set_pc_imm(target); self.emit8(0xC3);
                            self.emit_set_pc_imm(fallthrough); self.emit8(0xC3);
                        }
                        _ => return Err(CodegenError::UnsupportedOperand),
                    }
                    return Ok(());
                }
                Opcode::Ret => { self.load_state(X86Scratch::Rax, GuestReg::x(30)); self.mov_store(PC_OFFSET, X86Scratch::Rax, RegWidth::X64); self.emit8(0xC3); return Ok(()); }
                Opcode::Compare => return Err(CodegenError::UnsupportedOpcode(inst.opcode)),
                _ => return Err(CodegenError::UnsupportedOpcode(inst.opcode)),
            }
            if is_last {
                self.emit_set_pc_imm(block.guest_pc + block.byte_len());
                self.emit8(0xC3);
            }
        }
        if block.insts.is_empty() { self.emit_set_pc_imm(block.guest_pc); self.emit8(0xC3); }
        Ok(())
    }

    pub fn into_executable(self) -> Result<ExecutableCode, std::io::Error> { ExecutableCode::from_bytes(&self.bytes) }
}

#[derive(Clone, Copy)] enum X86Scratch { Rax, Rcx, Rdx }

pub struct ExecutableCode { mapping: Mmap, entry: GuestFn }

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
