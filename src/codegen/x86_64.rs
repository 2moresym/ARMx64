/// Direct x86-64 machine-code buffer.
#[derive(Debug, Default)]
pub struct CodeBuffer {
    pub bytes: Vec<u8>,
}

impl CodeBuffer {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn emit8(&mut self, byte: u8) {
        self.bytes.push(byte);
    }
}
