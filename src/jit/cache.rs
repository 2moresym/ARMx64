use std::sync::atomic::{AtomicPtr, Ordering};

/// Placeholder for the lock-free code-cache entry used by the JIT.
#[derive(Debug)]
pub struct CodeCache {
    pub entry: AtomicPtr<u8>,
}

impl Default for CodeCache {
    fn default() -> Self {
        Self {
            entry: AtomicPtr::new(std::ptr::null_mut()),
        }
    }
}

impl CodeCache {
    #[inline]
    pub fn load(&self) -> *mut u8 {
        self.entry.load(Ordering::Acquire)
    }

    #[inline]
    pub fn install(&self, ptr: *mut u8) {
        self.entry.store(ptr, Ordering::Release);
    }
}
