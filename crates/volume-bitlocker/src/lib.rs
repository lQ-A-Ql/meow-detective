//! Read-only BitLocker (BDE) volume decryption layer.
//!
//! This crate sits between a partition window reader and a filesystem reader:
//!
//! ```text
//! E01 / RAW -> PartitionWindowReader -> BitLockerReader -> NTFS / FAT / exFAT
//! ```
//!
//! # Stage 2a status
//!
//! The layer works end to end in isolation. Given a volume reader it reports the
//! cipher and the full protector inventory with no credential; given a password or
//! recovery password it produces a *verified* [`VolumeKeyPackage`]; and given that
//! package it presents the volume as plaintext through [`BitLockerReader`].
//!
//! It has no production callers yet. Wiring it into
//! `open_candidate_block_reader_with_lvm_cache` and resolving the eleven
//! `ImageFilesystemKind::BitLocker` gate points is Stage 2b, which needs the
//! runtime key registry.
//!
//! See `docs/bitlocker-volume-layer-design.md` for the staged plan and
//! `docs/bitlocker-dependency-decision.md` for upstream provenance.
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

mod bytes;
mod cipher;
mod diffuser;
mod error;
mod fingerprint;
mod guid;
mod header;
mod kdf;
mod layout;
mod metadata;
mod method;
mod protector;
mod reader;
mod secret;
mod unlock;

pub use cipher::SectorCipher;
pub use error::{BitLockerError, Result};
pub use fingerprint::MetadataFingerprint;
pub use guid::format_guid;
pub use header::{HeaderVariant, VolumeHeader};
pub use kdf::RecoveryPasswordError;
pub use layout::VolumeLayout;
pub use metadata::{FveMetadata, MetadataEntry};
pub use method::EncryptionMethod;
pub use protector::{ProtectorInventory, ProtectorKind};
pub use reader::{BitLockerReader, UnlockedVolume};
pub use secret::{Passphrase, VolumeKeyPackage};
pub use unlock::{
    read_volume_identity, unlock_with_password, unlock_with_recovery_password, VolumeIdentity,
};
