//! Read-only BitLocker (BDE) volume decryption layer.
//!
//! This crate sits between a partition window reader and a filesystem reader:
//!
//! ```text
//! E01 / RAW -> PartitionWindowReader -> BitLockerReader -> NTFS / FAT / exFAT
//! ```
//!
//! # Stage 0 status
//!
//! This is the frozen contract surface only. The types here define the boundary
//! that Stages 1-6 fill in; there is no metadata parser, no key derivation, and
//! no reader yet. See `docs/bitlocker-volume-layer-design.md` for the staged
//! plan and `docs/bitlocker-dependency-decision.md` for upstream provenance.
//!
//! # Boundaries that must not move
//!
//! - The original evidence image is opened read-only. This crate never writes to
//!   it, never mounts it, and never materializes a plaintext volume copy.
//! - Passwords and recovery passwords are process-lifetime secrets. They must not
//!   reach SQLite, job parameters, events, logs, error details, reports, or the
//!   frontend. Only a *verified* FVEK/tweak key package may be persisted.
//! - Encryption methods and key protectors are orthogonal. Never collapse them
//!   into one enumeration; see [`EncryptionMethod`] and [`ProtectorKind`].

#![forbid(unsafe_code)]

mod error;
mod method;
mod protector;
mod secret;

pub use error::{BitLockerError, Result};
pub use method::EncryptionMethod;
pub use protector::{ProtectorInventory, ProtectorKind};
pub use secret::{Passphrase, VolumeKeyPackage};
