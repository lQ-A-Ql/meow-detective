//! Bounded, read-only primitives for raw Windows x64 memory images.
//!
//! This crate deliberately provides only physical reads, page-table translation,
//! kernel-module discovery, tagged pool inventory, and bounded BitLocker key
//! candidate recognition. It has no Tauri or storage dependency, and it never
//! serializes secret-bearing data.

#![forbid(unsafe_code)]

mod aes_schedule;
mod bitlocker;
mod bootstrap;
mod error;
mod kernel;
mod physical;
mod pool;
mod x64;

pub use bitlocker::{
    scan_bitlocker_key_candidates, AesKeyBits, BitLockerKeyCandidate, BitLockerPoolTag,
};
pub use bootstrap::{
    discover_directory_table_base, find_processor_start_blocks, ProcessorStartBlock,
};
pub use error::{MemoryWindowsError, Result};
pub use kernel::{
    discover_kernel, find_kdbg_candidates, KdbgCandidate, KernelDiscovery, KernelModule,
    PeCodeViewIdentity,
};
pub use physical::RawMemoryImage;
pub use pool::{scan_pool_tag, PoolAllocation};
pub use x64::{is_canonical_address, X64AddressSpace};

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod tests;
