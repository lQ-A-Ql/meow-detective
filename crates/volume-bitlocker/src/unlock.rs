//! Metadata discovery and verified unlock API. Derived from bitlocker-core;
//! see ../NOTICE. Each implementation module owns one capability.
mod credentials;
mod error_rank;
mod evidence_io;
mod identity;
mod key_derivation;
mod verified;
pub use credentials::{
    unlock_volume_with_password, unlock_volume_with_password_for_identities,
    unlock_volume_with_recovery_password,
};
pub(crate) use error_rank::retain_preferred_error;
pub use identity::{read_volume_identities, read_volume_identity, VolumeIdentity};
pub(crate) use key_derivation::{
    checked_fvek_len, derive_key_package_from_vmk_bytes, derive_key_package_with_unwrap_key,
    stretch_salt_for_protector,
};
pub use verified::{restore_volume_from_persisted_key, VerifiedUnlock};

#[cfg(test)]
#[path = "../tests/unit/unlock/mod.rs"]
mod tests;
