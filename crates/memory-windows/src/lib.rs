//! Bounded, read-only primitives for raw Windows x64 memory images.
//!
//! This crate provides bounded physical reads, page-table translation, profiled
//! kernel-module discovery, and exact BitLocker object traversal. It has no
//! Tauri or storage dependency, performs no pool or AES-schedule scan, and never
//! serializes secret-bearing data.

#![forbid(unsafe_code)]

mod bootstrap;
mod error;
mod keyring_recovery;
mod physical;
mod targeted_kernel;
mod x64;

pub use bootstrap::{
    discover_directory_table_base, find_processor_start_blocks, ProcessorStartBlock,
};
pub use error::{MemoryWindowsError, Result};
pub use keyring_recovery::{
    recover_vmks_structurally, BitLockerMemoryProfile, BitLockerMemoryRecovery,
};
pub use physical::{PhysicalReadStats, RawMemoryImage};
pub use targeted_kernel::{
    discover_kernel_from_entry, discover_kernel_from_processor_start_block,
    LoadedModuleEntryLayout, TargetedCodeViewIdentity, TargetedKernelDiscovery,
    TargetedKernelIdentity, TargetedKernelLayoutProfile, TargetedKernelPeImage,
    TargetedKernelSearchLimits, TargetedKernelSearchReport,
};
pub use x64::{is_canonical_address, X64AddressSpace};

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod tests;
