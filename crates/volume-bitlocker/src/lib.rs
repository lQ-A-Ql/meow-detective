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
//! recovery password it produces a [`VerifiedUnlock`], whose cipher state presents
//! the volume as plaintext through [`BitLockerReader`].
//!
//! The read path is integrated through the application service's verified
//! unlock registry. Credential entry, import enumeration, and persistence of
//! verified key packages remain later stages; the crate itself never owns those
//! application concerns.
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
mod persisted_key;
mod protector;
mod reader;
mod recovery_password;
mod secret;
mod unlock;
mod unlock_vmk;

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
pub use recovery_password::{
    recover_recovery_password, recovery_password_protectors, RecoveredRecoveryPassword,
    RecoveryPasswordProtectorIdentity, RecoveryPasswordProvenance, RecoveryPasswordRecoveryError,
};
pub use secret::{Passphrase, PersistedKeyBlob, RecoveredVmk, RecoveryPassword};
pub use unlock::{
    read_volume_identities, read_volume_identity, restore_volume_from_persisted_key,
    unlock_volume_with_password, unlock_volume_with_recovery_password, VerifiedUnlock,
    VolumeIdentity,
};
pub use unlock_vmk::unlock_volume_with_recovered_vmk;
