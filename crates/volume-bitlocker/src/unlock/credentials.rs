//! Password and numerical recovery credential entry points.
use super::read_volume_identities;
use crate::kdf::{password_hash, recovery_key_hash, STRETCH_ITERATIONS};
use crate::metadata::{PROTECTION_PASSWORD, PROTECTION_RECOVERY};
use crate::unlock_dictionary;
use crate::{BitLockerError, Passphrase, ProtectorKind, Result, VerifiedUnlock, VolumeIdentity};
use std::io::{Read, Seek};
/// Unlocks a volume with a password, trying every complete metadata copy through
/// VMK unwrap, FVEK unwrap, and cipher construction before failing.
pub fn unlock_volume_with_password<R: Read + Seek>(
    reader: &mut R,
    password: &Passphrase,
) -> Result<VerifiedUnlock> {
    let hash = password_hash(password.expose_for_derivation());
    unlock_volume_with_hash(
        reader,
        ProtectorKind::Password,
        PROTECTION_PASSWORD,
        &hash,
        STRETCH_ITERATIONS,
    )
}

/// Unlocks a volume using identities that were already read from its metadata.
///
/// Dictionary attacks try many credentials against one immutable metadata set.
/// Keeping the identities outside the candidate loop avoids seeking and parsing
/// the evidence for every password while preserving the same authenticated VMK
/// and FVEK checks as [`unlock_volume_with_password`].
pub fn unlock_volume_with_password_for_identities(
    identities: &[VolumeIdentity],
    password: &Passphrase,
) -> Result<VerifiedUnlock> {
    let hash = password_hash(password.expose_for_derivation());
    unlock_dictionary::unlock_identities_with_hash(
        identities,
        ProtectorKind::Password,
        PROTECTION_PASSWORD,
        &hash,
        STRETCH_ITERATIONS,
    )
}

/// Unlocks a volume with a 48-digit recovery password, trying every complete
/// metadata copy before failing.
pub fn unlock_volume_with_recovery_password<R: Read + Seek>(
    reader: &mut R,
    recovery: &Passphrase,
) -> Result<VerifiedUnlock> {
    let hash = recovery_key_hash(recovery.expose_for_derivation())
        .map_err(|_| BitLockerError::CredentialRejected)?;
    unlock_volume_with_hash(
        reader,
        ProtectorKind::RecoveryPassword,
        PROTECTION_RECOVERY,
        &hash,
        STRETCH_ITERATIONS,
    )
}

pub(super) fn unlock_volume_with_hash<R: Read + Seek>(
    reader: &mut R,
    protector: ProtectorKind,
    protection_code: u16,
    credential_hash: &[u8; 32],
    iterations: u64,
) -> Result<VerifiedUnlock> {
    let identities = read_volume_identities(reader)?;
    unlock_dictionary::unlock_identities_with_hash(
        &identities,
        protector,
        protection_code,
        credential_hash,
        iterations,
    )
}
