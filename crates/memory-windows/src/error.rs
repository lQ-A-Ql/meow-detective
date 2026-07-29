use std::io;

#[derive(Debug, thiserror::Error)]
pub enum MemoryWindowsError {
    #[error("memory image is empty")]
    EmptyImage,
    #[error("memory image read failed at physical offset {offset:#X}: {source}")]
    PhysicalRead {
        offset: u64,
        #[source]
        source: io::Error,
    },
    #[error("memory image range {offset:#X}..{end:#X} exceeds length {length:#X}")]
    PhysicalOutOfBounds { offset: u64, end: u64, length: u64 },
    #[error("virtual address {address:#X} is not a canonical four-level x64 address")]
    NonCanonicalAddress { address: u64 },
    #[error("page-table entry is not present for virtual address {address:#X}")]
    PageNotPresent { address: u64 },
    #[error("page-table entry for virtual address {address:#X} points outside the memory image")]
    InvalidPageFrame { address: u64 },
    #[error("no valid KDBG candidate was found in the memory image")]
    KdbgNotFound,
    #[error("no valid x64 processor start block was found in the low-memory region")]
    ProcessorStartBlockNotFound,
    #[error("no kernel page-table root could be validated from the KDBG candidates")]
    KernelAddressSpaceNotFound,
    #[error("kernel page-table traversal exceeded the bounded table budget")]
    PageTableBudgetExceeded,
    #[error("kernel module list is malformed or cyclic")]
    MalformedModuleList,
    #[error("kernel module PE headers are malformed")]
    MalformedPe,
}

pub type Result<T> = std::result::Result<T, MemoryWindowsError>;
