use std::io;
use std::ptr;

/// Fixed virtual address used as the base of the guest address space.
///
/// The translated code can therefore turn a guest address into a host address
/// with a single base addition. Linux/x86-64 is the first supported runtime.
pub const HOST_BASE: usize = 0x0000_4000_0000_0000;

#[derive(Debug)]
pub struct GuestMemory {
    base: usize,
    size: usize,
}

impl GuestMemory {
    /// Reserve a fixed guest address space with read/write permissions.
    pub fn map(size: usize) -> io::Result<Self> {
        if size == 0 { return Err(io::Error::new(io::ErrorKind::InvalidInput, "guest memory size is zero")); }
        let ptr = unsafe {
            libc::mmap(
                HOST_BASE as *mut libc::c_void,
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_FIXED_NOREPLACE,
                -1,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        if ptr as usize != HOST_BASE {
            unsafe { libc::munmap(ptr, size); }
            return Err(io::Error::new(io::ErrorKind::Other, "kernel did not honor fixed guest address"));
        }
        Ok(Self { base: HOST_BASE, size })
    }

    #[inline] pub const fn base(&self) -> usize { self.base }
    #[inline] pub const fn size(&self) -> usize { self.size }

    #[inline]
    pub const fn host_address(&self, guest_address: usize) -> usize { self.base + guest_address }

    #[inline]
    pub fn write_u32(&mut self, guest_address: usize, value: u32) {
        assert!(guest_address.checked_add(4).is_some_and(|end| end <= self.size));
        unsafe { ptr::write_unaligned((self.base + guest_address) as *mut u32, value); }
    }

    #[inline]
    pub fn write_u64(&mut self, guest_address: usize, value: u64) {
        assert!(guest_address.checked_add(8).is_some_and(|end| end <= self.size));
        unsafe { ptr::write_unaligned((self.base + guest_address) as *mut u64, value); }
    }

    #[inline]
    pub fn read_u32(&self, guest_address: usize) -> u32 {
        assert!(guest_address.checked_add(4).is_some_and(|end| end <= self.size));
        unsafe { ptr::read_unaligned((self.base + guest_address) as *const u32) }
    }

    #[inline]
    pub fn read_u64(&self, guest_address: usize) -> u64 {
        assert!(guest_address.checked_add(8).is_some_and(|end| end <= self.size));
        unsafe { ptr::read_unaligned((self.base + guest_address) as *const u64) }
    }
}

impl Drop for GuestMemory {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.base as *mut libc::c_void, self.size); }
    }
}
