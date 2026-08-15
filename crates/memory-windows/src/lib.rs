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

pub use bootstrap::{discover_directory_table_base, ProcessorStartBlock};
pub use error::MemoryWindowsError;
pub use keyring_recovery::{
    recover_vmks_structurally, resolve_profile_for_image, BitLockerMemoryProfile,
    BitLockerMemoryRecovery,
};
pub use physical::{PhysicalReadStats, RawMemoryImage};
pub use targeted_kernel::TargetedKernelSearchLimits;
pub use x64::X64AddressSpace;

pub(crate) use error::Result;
pub(crate) use x64::is_canonical_address;

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod tests;
