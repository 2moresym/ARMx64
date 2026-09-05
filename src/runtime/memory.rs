/// Guest memory model. The fixed-offset mapping will be added here.
#[derive(Debug)]
pub struct GuestMemory {
    pub base: usize,
    pub size: usize,
}

impl GuestMemory {
    pub const fn new(base: usize, size: usize) -> Self {
        Self { base, size }
    }

    #[inline]
    pub const fn host_address(&self, guest_address: usize) -> usize {
        self.base + guest_address
    }
}
