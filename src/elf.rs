use std::fs;
use std::io;
use std::path::Path;

use object::{Object, ObjectSegment};

use crate::runtime::GuestMemory;

const PAGE_SIZE: usize = 4096;
const DEFAULT_STACK_SIZE: usize = 8 * 1024 * 1024;
const ADDRESS_SPACE_SLACK: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct LoadSegment {
    pub address: u64,
    pub size: u64,
}

#[derive(Debug)]
pub struct LoadedElf {
    pub entry: u64,
    pub memory: GuestMemory,
    pub segments: Vec<LoadSegment>,
}

#[derive(Debug)]
pub enum ElfLoadError {
    Io(io::Error),
    Parse(object::read::Error),
    UnsupportedArchitecture(object::Architecture),
    InvalidSegment { address: u64, size: u64 },
}

impl From<io::Error> for ElfLoadError {
    fn from(value: io::Error) -> Self { Self::Io(value) }
}

impl From<object::read::Error> for ElfLoadError {
    fn from(value: object::read::Error) -> Self { Self::Parse(value) }
}

impl std::fmt::Display for ElfLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Parse(e) => write!(f, "ELF parse error: {e}"),
            Self::UnsupportedArchitecture(a) => write!(f, "unsupported ELF architecture: {a:?}"),
            Self::InvalidSegment { address, size } => write!(f, "invalid PT_LOAD segment at {address:#x} ({size:#x} bytes)"),
        }
    }
}

impl std::error::Error for ElfLoadError {}

/// Load an AArch64 ELF into the ARMx64 guest address space.
///
/// This is deliberately a loader, not a Linux process launcher yet: it maps
/// PT_LOAD contents, zero-fills BSS, and exposes the ELF entry point. Dynamic
/// linking, argv/envp, auxiliary vectors, and syscalls come later.
pub fn load(path: impl AsRef<Path>) -> Result<LoadedElf, ElfLoadError> {
    let bytes = fs::read(path)?;
    load_bytes(&bytes)
}

pub fn load_bytes(bytes: &[u8]) -> Result<LoadedElf, ElfLoadError> {
    let file = object::File::parse(bytes)?;
    if file.architecture() != object::Architecture::Aarch64 {
        return Err(ElfLoadError::UnsupportedArchitecture(file.architecture()));
    }

    let mut max_end = 0usize;
    let mut segments = Vec::new();

    for segment in file.segments() {
        let address = segment.address();
        let size = segment.size();
        if size == 0 { continue; }
        let end = address.checked_add(size).ok_or(ElfLoadError::InvalidSegment { address, size })?;
        let end = usize::try_from(end).map_err(|_| ElfLoadError::InvalidSegment { address, size })?;
        max_end = max_end.max(end);
        segments.push(LoadSegment { address, size });
    }

    if segments.is_empty() {
        return Err(ElfLoadError::InvalidSegment { address: 0, size: 0 });
    }

    let image_end = align_up(max_end, PAGE_SIZE).ok_or(ElfLoadError::InvalidSegment {
        address: 0,
        size: max_end as u64,
    })?;
    let memory_size = image_end
        .checked_add(DEFAULT_STACK_SIZE)
        .and_then(|n| n.checked_add(ADDRESS_SPACE_SLACK))
        .ok_or(ElfLoadError::InvalidSegment { address: 0, size: max_end as u64 })?;

    let mut memory = GuestMemory::map(memory_size)?;
    for segment in file.segments() {
        let address = usize::try_from(segment.address()).map_err(|_| ElfLoadError::InvalidSegment {
            address: segment.address(),
            size: segment.size(),
        })?;
        let data = segment.data()?;
        if data.len() > usize::try_from(segment.size()).unwrap_or(usize::MAX) {
            return Err(ElfLoadError::InvalidSegment { address: segment.address(), size: segment.size() });
        }
        memory.write_bytes(address, data).map_err(ElfLoadError::Io)?;
        // mmap() supplies zero-filled pages, so the remainder of p_memsz is BSS.
    }

    Ok(LoadedElf {
        entry: file.entry(),
        memory,
        segments,
    })
}

#[inline]
fn align_up(value: usize, alignment: usize) -> Option<usize> {
    value.checked_add(alignment - 1).map(|v| v & !(alignment - 1))
}
