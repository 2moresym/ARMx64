use yaxpeax_arch::{Decoder, U8Reader};
use yaxpeax_arm::armv8::a64::{DecodeError, InstDecoder, Instruction};

/// A raw AArch64 instruction word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct A64Inst(pub u32);

impl A64Inst {
    #[inline]
    pub const fn to_le_bytes(self) -> [u8; 4] {
        self.0.to_le_bytes()
    }
}

/// Decode one little-endian AArch64 instruction.
#[inline]
pub fn decode(inst: A64Inst) -> Result<Instruction, DecodeError> {
    let bytes = inst.to_le_bytes();
    let mut reader = U8Reader::new(&bytes);
    InstDecoder::default().decode(&mut reader)
}
