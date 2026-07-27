//! Read-only BitLocker (BDE) volume decryption layer.
//!
//! This crate sits between a partition window reader and a filesystem reader:
//!
//! ```text
//! E01 / RAW -> PartitionWindowReader -> BitLockerReader -> NTFS / FAT / exFAT
//! ```
//!
//! # Stage 1 status
//!
//! Metadata parsing and key derivation are implemented: given a volume reader
//! this crate reports the cipher and the full protector inventory with no
//! credential, and given a password or recovery password it produces a *verified*
//! [`VolumeKeyPackage`]. It does not decrypt sectors yet — the sector cipher and
//! the `Read + Seek` plaintext view are Stage 2, so nothing here is wired into
//! the evidence read path.
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
mod error;
mod fingerprint;
mod guid;
mod header;
mod kdf;
mod metadata;
mod method;
mod protector;
mod secret;
mod unlock;

pub use error::{BitLockerError, Result};
pub use fingerprint::MetadataFingerprint;
pub use guid::format_guid;
pub use header::{HeaderVariant, VolumeHeader};
pub use kdf::RecoveryPasswordError;
pub use metadata::{FveMetadata, MetadataEntry};
pub use method::EncryptionMethod;
pub use protector::{ProtectorInventory, ProtectorKind};
pub use secret::{Passphrase, VolumeKeyPackage};
pub use unlock::{
    read_volume_identity, unlock_with_password, unlock_with_recovery_password, VolumeIdentity,
};
