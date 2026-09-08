use crate::ir::{Block, GuestReg, IRInst, Opcode, Operand, RegKind, RegWidth, ShiftKind, FLAG_WRITES_NZCV};
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
            (X86Scratch::Rsi, RegWidth::X64) => { self.emit8(0x48); self.emit8(0xBE); self.emit64(value); }
            (X86Scratch::Rax, RegWidth::W32) => { self.emit8(0xB8); self.emit32(value as u32); }
            (X86Scratch::Rcx, RegWidth::W32) => { self.emit8(0xB9); self.emit32(value as u32); }
            (X86Scratch::Rdx, RegWidth::W32) => { self.emit8(0xBA); self.emit32(value as u32); }
            (X86Scratch::Rsi, RegWidth::W32) => { self.emit8(0xBE); self.emit32(value as u32); }
        }
    }

    #[inline]
    fn mov_load(&mut self, reg: X86Scratch, disp: i32, width: RegWidth) {
        self.rex(width == RegWidth::X64); self.emit8(0x8B);
        self.emit8(match reg { X86Scratch::Rax => 0x87, X86Scratch::Rcx => 0x8F, X86Scratch::Rdx => 0x97, X86Scratch::Rsi => 0xB7 }); self.emit32(disp as u32);
    }

    #[inline]
    fn mov_store(&mut self, disp: i32, reg: X86Scratch, width: RegWidth) {
        self.rex(width == RegWidth::X64); self.emit8(0x89);
        self.emit8(match reg { X86Scratch::Rax => 0x87, X86Scratch::Rcx => 0x8F, X86Scratch::Rdx => 0x97, X86Scratch::Rsi => 0xB7 }); self.emit32(disp as u32);
    }

    #[inline]
    fn xor(&mut self, dst: X86Scratch, src: X86Scratch, width: RegWidth) {
        self.rex(width == RegWidth::X64); self.emit8(0x31);
        self.emit8(match (dst, src) {
            (X86Scratch::Rax, X86Scratch::Rax) => 0xC0,
            (X86Scratch::Rax, X86Scratch::Rcx) => 0xC8,
            (X86Scratch::Rcx, X86Scratch::Rax) => 0xC1,
            (X86Scratch::Rcx, X86Scratch::Rcx) => 0xC9,
            (X86Scratch::Rdx, X86Scratch::Rdx) => 0xD2,
            (X86Scratch::Rsi, X86Scratch::Rsi) => 0xF6,
            _ => unreachable!(),
        });
    }

    #[inline]
    fn binop(&mut self, op: Opcode, width: RegWidth) {
        self.rex(width == RegWidth::X64);
        self.emit8(match op { Opcode::Add => 0x01, Opcode::Sub => 0x29, Opcode::And => 0x21, Opcode::Orr => 0x09, Opcode::Eor => 0x31, _ => unreachable!() });
        self.emit8(0xC8);
    }

    #[inline]
    fn emit_nzcv_from_x86_flags(&mut self, op: Opcode) {
        // Capture flags before any arithmetic/logic instruction can overwrite EFLAGS.
        self.emit8(0x0F); self.emit8(0x98); self.emit8(0xC2); // sets dl = N
        self.emit8(0x0F); self.emit8(0x94); self.emit8(0xC1); // sete cl = Z
        match op {
            Opcode::Add => {
                self.emit8(0x0F); self.emit8(0x92); self.emit8(0xC0); // setb al = C
                self.emit8(0x40); self.emit8(0x0F); self.emit8(0x90); self.emit8(0xC6); // seto sil = V
            }
            Opcode::Sub => {
                self.emit8(0x0F); self.emit8(0x93); self.emit8(0xC0); // setae al = ARM C (no borrow)
                self.emit8(0x40); self.emit8(0x0F); self.emit8(0x90); self.emit8(0xC6); // seto sil = V
            }
            Opcode::And => {
                self.emit8(0x31); self.emit8(0xC0); // C=0
                self.emit8(0x31); self.emit8(0xF6); // V=0
            }
            _ => unreachable!(),
        }

        // Materialize ARM NZCV in bits 31:28.
        self.emit8(0x0F); self.emit8(0xB6); self.emit8(0xD2); // movzx edx, dl
        self.emit8(0xC1); self.emit8(0xE2); self.emit8(0x1F); // shl edx,31
        self.emit8(0x0F); self.emit8(0xB6); self.emit8(0xC9); // movzx ecx, cl
        self.emit8(0xC1); self.emit8(0xE1); self.emit8(0x1E); // shl ecx,30
        self.emit8(0x09); self.emit8(0xCA); // or edx,ecx
        self.emit8(0x0F); self.emit8(0xB6); self.emit8(0xC0); // movzx eax,al
        self.emit8(0xC1); self.emit8(0xE0); self.emit8(0x1D); // shl eax,29
        self.emit8(0x09); self.emit8(0xC2); // or edx,eax
        self.emit8(0x40); self.emit8(0x0F); self.emit8(0xB6); self.emit8(0xF6); // movzx esi,sil
        self.emit8(0xC1); self.emit8(0xE6); self.emit8(0x1C); // shl esi,28
        self.emit8(0x09); self.emit8(0xF2); // or edx,esi
        self.mov_store(NZCV_OFFSET, X86Scratch::Rdx, RegWidth::W32);
    }

    #[inline]
    fn shift_imm(&mut self, reg: X86Scratch, kind: ShiftKind, amount: u8, width: RegWidth) {
        if amount == 0 { return; }
        self.rex(width == RegWidth::X64); self.emit8(0xC1);
        let rm = match (reg, kind) { (X86Scratch::Rax, ShiftKind::Lsl) => 0xE0, (X86Scratch::Rax, ShiftKind::Lsr) => 0xE8, (X86Scratch::Rax, ShiftKind::Asr) => 0xF8, (X86Scratch::Rax, ShiftKind::Ror) => 0xC8, (X86Scratch::Rcx, ShiftKind::Lsl) => 0xE1, (X86Scratch::Rcx, ShiftKind::Lsr) => 0xE9, (X86Scratch::Rcx, ShiftKind::Asr) => 0xF9, (X86Scratch::Rcx, ShiftKind::Ror) => 0xC9, (X86Scratch::Rdx, ShiftKind::Lsl) => 0xE2, (X86Scratch::Rdx, ShiftKind::Lsr) => 0xEA, (X86Scratch::Rdx, ShiftKind::Asr) => 0xFA, (X86Scratch::Rdx, ShiftKind::Ror) => 0xCA, (X86Scratch::Rsi, ShiftKind::Lsl) => 0xE6, (X86Scratch::Rsi, ShiftKind::Lsr) => 0xEE, (X86Scratch::Rsi, ShiftKind::Asr) => 0xFE, (X86Scratch::Rsi, ShiftKind::Ror) => 0xCE };
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
    fn emit_branch_cond(&mut self, target: u64, condition: u64, fallthrough: u64) -> Result<(), CodegenError> {
        if condition > 15 { return Err(CodegenError::UnsupportedOperand); }
        const SET_PC_RET_LEN: u8 = 18;
        const COND_TABLES: [u16; 16] = build_condition_tables();
        self.mov_load(X86Scratch::Rax, NZCV_OFFSET, RegWidth::W32);
        self.emit8(0xC1); self.emit8(0xE8); self.emit8(0x1C);
        self.mov_imm(X86Scratch::Rcx, COND_TABLES[condition as usize] as u64, RegWidth::W32);
        self.emit8(0x66); self.emit8(0x0F); self.emit8(0xA3); self.emit8(0xC1);
        self.emit8(0x73); self.emit8(SET_PC_RET_LEN);
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
                    self.load_operand(inst.b, X86Scratch::Rax, dest.width)?; self.load_operand(inst.c, X86Scratch::Rcx, dest.width)?;
                    self.binop(inst.opcode, dest.width);
                    if inst.flags != 0 { self.store_state(dest, X86Scratch::Rax); self.emit_nzcv_from_x86_flags(inst.opcode); }
                    else { self.store_state(dest, X86Scratch::Rax); }
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
                        (Operand::Imm(cond), Operand::None) => self.emit_branch_cond(target, cond, fallthrough)?,
                        (Operand::Reg(reg), Operand::Imm(kind)) if kind <= 1 => {
                            self.load_state(X86Scratch::Rax, reg);
                            self.rex(reg.width == RegWidth::X64); self.emit8(0x85); self.emit8(0xC0);
                            self.emit8(if kind == 0 { 0x75 } else { 0x74 }); self.emit8(SET_PC_RET_LEN);
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
            if is_last { self.emit_set_pc_imm(block.guest_pc + block.byte_len()); self.emit8(0xC3); }
        }
        if block.insts.is_empty() { self.emit_set_pc_imm(block.guest_pc); self.emit8(0xC3); }
        Ok(())
    }

    pub fn into_executable(self) -> Result<ExecutableCode, std::io::Error> { ExecutableCode::from_bytes(&self.bytes) }
}

const fn condition_holds(cond: usize, nzcv: usize) -> bool {
    let n = (nzcv & 8) != 0; let z = (nzcv & 4) != 0; let c = (nzcv & 2) != 0; let v = (nzcv & 1) != 0;
    match cond { 0 => z, 1 => !z, 2 => c, 3 => !c, 4 => n, 5 => !n, 6 => v, 7 => !v, 8 => c && !z, 9 => !c || z, 10 => n == v, 11 => n != v, 12 => !z && (n == v), 13 => z || (n != v), 14 => true, 15 => false, _ => false }
}

const fn condition_truth_table(cond: usize) -> u16 {
    let mut table = 0u16; let mut nzcv = 0usize;
    while nzcv < 16 { if condition_holds(cond, nzcv) { table |= 1u16 << nzcv; } nzcv += 1; }
    table
}

const fn build_condition_tables() -> [u16; 16] {
    let mut tables = [0u16; 16]; let mut cond = 0usize;
    while cond < 16 { tables[cond] = condition_truth_table(cond); cond += 1; }
    tables
}

#[derive(Clone, Copy)] enum X86Scratch { Rax, Rcx, Rdx, Rsi }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn run(block: &Block, state: &mut GuestState) {
        let mut buffer = CodeBuffer::new();
        buffer.emit_block(block).expect("codegen");
        let code = buffer.into_executable().expect("executable mapping");
        unsafe { (code.entry())(state as *mut GuestState); }
    }

    fn arithmetic_block(opcode: Opcode, lhs: Operand, rhs: Operand, width: RegWidth) -> Block {
        let mut block = Block::at(0x1000);
        block.push(IRInst { opcode, flags: FLAG_WRITES_NZCV, a: Operand::Reg(GuestReg { num: 0, width, kind: RegKind::General }), b: lhs, c: rhs });
        block
    }

    #[test]
    fn adds_materializes_nzcv() {
        let mut state = GuestState::new(); state.gpr[0] = u64::MAX;
        let block = arithmetic_block(Opcode::Add, Operand::Reg(GuestReg::x(0)), Operand::Imm(1), RegWidth::X64);
        run(&block, &mut state); assert_eq!(state.gpr[0], 0); assert_eq!(state.nzcv, 0x6000_0000);
    }

    #[test]
    fn subs_materializes_no_borrow_carry() {
        let mut state = GuestState::new(); state.gpr[0] = 0;
        let block = arithmetic_block(Opcode::Sub, Operand::Reg(GuestReg::x(0)), Operand::Imm(1), RegWidth::X64);
        run(&block, &mut state); assert_eq!(state.gpr[0], u64::MAX); assert_eq!(state.nzcv, 0x8000_0000);
    }

    #[test]
    fn ands_sets_only_nz() {
        let mut state = GuestState::new(); state.gpr[0] = 0x8000_0000_0000_0000;
        let block = arithmetic_block(Opcode::And, Operand::Reg(GuestReg::x(0)), Operand::Imm(u64::MAX), RegWidth::X64);
        run(&block, &mut state); assert_eq!(state.nzcv, 0x8000_0000);
    }

    #[test]
    fn conditional_branches_cover_all_conditions() {
        for cond in 0u64..16 { for nzcv_nibble in 0usize..16 {
            let mut state = GuestState::new(); state.nzcv = (nzcv_nibble as u32) << 28;
            let mut block = Block::at(0x1000);
            block.push(IRInst { opcode: Opcode::BranchCond, flags: 0, a: Operand::GuestPc(0x2000), b: Operand::Imm(cond), c: Operand::None });
            run(&block, &mut state);
            let expected = condition_holds(cond as usize, nzcv_nibble);
            assert_eq!(state.pc, if expected { 0x2000 } else { 0x1004 }, "cond={cond} nzcv={nzcv_nibble:x}");
        }}
    }

    #[test]
    fn cbz_and_cbnz_w32_use_low_32_bits() {
        let mut cbz = Block::at(0x1000);
        cbz.push(IRInst { opcode: Opcode::BranchCond, flags: 0, a: Operand::GuestPc(0x2000), b: Operand::Reg(GuestReg::w(0)), c: Operand::Imm(0) });
        let mut state = GuestState::new(); state.gpr[0] = 0x1_0000_0000; run(&cbz, &mut state); assert_eq!(state.pc, 0x2000);
        let mut cbnz = Block::at(0x1000);
        cbnz.push(IRInst { opcode: Opcode::BranchCond, flags: 0, a: Operand::GuestPc(0x2000), b: Operand::Reg(GuestReg::w(0)), c: Operand::Imm(1) });
        state.gpr[0] = 0x1_0000_0000; run(&cbnz, &mut state); assert_eq!(state.pc, 0x1004);
    }
}
