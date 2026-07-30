//! Metadata discovery and credential-to-key derivation.
//!
//! Derived from `bitlocker-core`'s `volume` module unlock path (see `../NOTICE`).
//!
//! This module answers two questions and nothing else:
//!
//! 1. What is on this volume — which cipher, which protectors? Answerable with no
//!    credential, and the forensically useful answer even when the volume stays
//!    locked.
//! 2. Given a credential, what is the verified key package? Producing it does not
//!    decrypt anything; the sector reader is Stage 2.

use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

use zeroize::Zeroizing;

use crate::bytes::le_u32;
use crate::error::{BitLockerError, Result};
use crate::header::VolumeHeader;
use crate::kdf::{
    aes_ccm_unwrap, password_hash, recovery_key_hash, stretch_key_n, STRETCH_ITERATIONS,
};
use crate::metadata::{
    FveMetadata, MetadataEntry, BLOCK_HEADER_LEN, MAX_METADATA_ENTRIES_LEN, METADATA_HEADER_LEN,
    PROTECTION_PASSWORD, PROTECTION_RECOVERY, VALUE_TYPE_AES_CCM, VALUE_TYPE_STRETCH,
};
use crate::protector::ProtectorKind;
use crate::reader::UnlockedVolume;
use crate::secret::{Passphrase, PersistedKeyBlob, VolumeKeyPackage};

/// The 512-byte volume header sector.
const HEADER_LEN: usize = 512;
/// Enough bytes to validate both fixed headers and obtain the metadata size.
const METADATA_PREFIX_LEN: usize = BLOCK_HEADER_LEN + METADATA_HEADER_LEN;

/// What a locked volume reveals without any credential.
#[derive(Debug, Clone)]
pub struct VolumeIdentity {
    /// The parsed metadata block.
    pub metadata: FveMetadata,
    /// Bytes per sector, from the volume header.
    pub bytes_per_sector: u16,
}

/// A volume identity paired with immutable cipher state produced only after both
/// AES-CCM authentication checks succeed.
pub struct VerifiedUnlock {
    identity: VolumeIdentity,
    volume: Arc<UnlockedVolume>,
    keys: VolumeKeyPackage,
}

impl VerifiedUnlock {
    /// The metadata copy that produced the verified keys.
    #[must_use]
    pub fn identity(&self) -> &VolumeIdentity {
        &self.identity
    }

    /// Transfers the verified identity and shared plaintext-volume state to the
    /// runtime registry.
    #[must_use]
    pub fn into_unlocked_volume(self) -> (VolumeIdentity, Arc<UnlockedVolume>) {
        (self.identity, self.volume)
    }

    /// Borrows the verified plaintext-volume capability for an additional
    /// read-only validation reader without exposing raw key bytes.
    #[must_use]
    pub fn shared_unlocked_volume(&self) -> Arc<UnlockedVolume> {
        Arc::clone(&self.volume)
    }

    /// Exports the verified key material into the bounded v1 storage envelope.
    /// The raw FVEK remains inaccessible to application and transport layers.
    #[must_use]
    pub fn persisted_key_blob(&self) -> PersistedKeyBlob {
        crate::persisted_key::encode(&self.identity, &self.keys)
    }

    pub(crate) fn from_restored(
        identity: VolumeIdentity,
        volume: Arc<UnlockedVolume>,
        keys: VolumeKeyPackage,
    ) -> Self {
        Self {
            identity,
            volume,
            keys,
        }
    }
}

/// Rebuilds verified runtime state from a persisted key package after strict
/// identity and envelope validation.
pub fn restore_volume_from_persisted_key(
    identity: VolumeIdentity,
    blob: PersistedKeyBlob,
) -> Result<VerifiedUnlock> {
    crate::persisted_key::restore(identity, blob)
}

/// Reads the volume header and the first valid FVE metadata block.
///
/// Tries every non-zero metadata offset before failing, because the three copies
/// exist precisely so a damaged block does not lose the volume.
///
/// A successful return is also what confirms the volume really is BitLocker: a
/// `MSWIN4.1` header alone is ambiguous with plain FAT, and only the `-FVE-FS-`
/// metadata block settles it.
///
/// # Errors
///
/// [`BitLockerError::MetadataUnreadable`] when the header signature is absent or
/// no candidate offset yields a valid block; [`BitLockerError::EvidenceRead`] when
/// the underlying reader fails.
pub fn read_volume_identity<R: Read + Seek>(reader: &mut R) -> Result<VolumeIdentity> {
    read_volume_identities(reader)?
        .into_iter()
        .next()
        .ok_or_else(|| BitLockerError::MetadataUnreadable {
            reason: "the volume header contains no non-zero metadata offsets".to_string(),
        })
}

/// Reads every structurally valid FVE metadata copy reachable from the volume
/// header. A failed seek, short read, or malformed copy is isolated to that copy.
///
/// # Errors
///
/// Returns an evidence-read error when the volume header itself is unavailable,
/// or metadata-unreadable after all non-zero copies fail validation.
pub fn read_volume_identities<R: Read + Seek>(reader: &mut R) -> Result<Vec<VolumeIdentity>> {
    let mut header_sector = [0u8; HEADER_LEN];
    read_exact_at(reader, 0, &mut header_sector)?;
    let header = VolumeHeader::parse(&header_sector)?;

    let mut offsets = header
        .fve_metadata_offsets
        .into_iter()
        .filter(|offset| *offset != 0)
        .collect::<Vec<_>>();
    offsets.dedup();
    let mut identities = Vec::new();
    let mut failures = Vec::new();
    let mut index = 0usize;
    while index < offsets.len() {
        let offset = offsets[index];
        index += 1;
        match read_metadata_copy(reader, offset, header.bytes_per_sector) {
            Ok(metadata) => {
                for discovered in metadata.metadata_offsets {
                    if discovered != 0 && !offsets.contains(&discovered) {
                        offsets.push(discovered);
                    }
                }
                identities.push(VolumeIdentity {
                    metadata,
                    bytes_per_sector: header.bytes_per_sector,
                });
            }
            Err(error) => failures.push(format!("{offset:#X}: {error}")),
        }
    }

    if identities.is_empty() {
        return Err(BitLockerError::MetadataUnreadable {
            reason: format!(
                "no complete v2 metadata block at candidate offsets {offsets:?}; {}",
                failures.join("; ")
            ),
        });
    }
    Ok(identities)
}

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

fn unlock_volume_with_hash<R: Read + Seek>(
    reader: &mut R,
    protector: ProtectorKind,
    protection_code: u16,
    credential_hash: &[u8; 32],
    iterations: u64,
) -> Result<VerifiedUnlock> {
    let identities = read_volume_identities(reader)?;
    let mut preferred_error = None;
    for identity in identities {
        match derive_key_package(
            &identity.metadata,
            protector,
            protection_code,
            credential_hash,
            iterations,
        )
        .and_then(|keys| {
            let volume = UnlockedVolume::new(&identity.metadata, &keys)?;
            Ok((keys, volume))
        }) {
            Ok((keys, volume)) => {
                return Ok(VerifiedUnlock {
                    identity,
                    volume: Arc::new(volume),
                    keys,
                });
            }
            Err(error) => retain_preferred_error(&mut preferred_error, error),
        }
    }
    Err(
        preferred_error.unwrap_or_else(|| BitLockerError::MetadataUnreadable {
            reason: "no metadata copy produced a verified volume key".to_string(),
        }),
    )
}

pub(crate) fn retain_preferred_error(slot: &mut Option<BitLockerError>, candidate: BitLockerError) {
    let candidate_rank = unlock_error_rank(&candidate);
    let current_rank = slot.as_ref().map(unlock_error_rank).unwrap_or(0);
    if candidate_rank >= current_rank {
        *slot = Some(candidate);
    }
}

fn unlock_error_rank(error: &BitLockerError) -> u8 {
    match error {
        BitLockerError::CredentialRejected => 4,
        BitLockerError::UnsupportedProtector { .. } => 3,
        BitLockerError::UnsupportedEncryptionMethod { .. } => 2,
        _ => 1,
    }
}

fn read_metadata_copy<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    bytes_per_sector: u16,
) -> Result<FveMetadata> {
    let mut prefix = [0u8; METADATA_PREFIX_LEN];
    read_exact_at(reader, offset, &mut prefix)?;
    let metadata_size = le_u32(&prefix, BLOCK_HEADER_LEN) as usize;
    let entries_len = metadata_size
        .checked_sub(METADATA_HEADER_LEN)
        .ok_or_else(|| BitLockerError::MetadataUnreadable {
            reason: format!("metadata copy at {offset:#X} has a header smaller than 48 bytes"),
        })?;
    if entries_len > MAX_METADATA_ENTRIES_LEN {
        return Err(BitLockerError::MetadataUnreadable {
            reason: format!(
                "metadata copy at {offset:#X} declares {entries_len} entry bytes; maximum is {MAX_METADATA_ENTRIES_LEN}"
            ),
        });
    }
    let total_len = BLOCK_HEADER_LEN.checked_add(metadata_size).ok_or_else(|| {
        BitLockerError::MetadataUnreadable {
            reason: format!("metadata copy at {offset:#X} has an overflowing size"),
        }
    })?;
    let mut block = vec![0u8; total_len];
    read_exact_at(reader, offset, &mut block)?;
    FveMetadata::parse(&block, bytes_per_sector).ok_or_else(|| BitLockerError::MetadataUnreadable {
        reason: format!("metadata copy at {offset:#X} failed strict v2 validation"),
    })
}

/// Derives and verifies the key package for one protector.
///
/// The steps are: locate the VMK entry for this protector, stretch the credential
/// hash with that VMK's salt, AES-CCM-unwrap the VMK, then AES-CCM-unwrap the
/// FVEK with it. Both tag checks must pass, which is what makes the returned
/// package *verified* rather than merely derived.
///
/// # Errors
///
/// - [`BitLockerError::UnsupportedEncryptionMethod`] when the cipher has no
///   validated decrypt path, checked before any credential work so an unsupported
///   volume fails fast instead of after a one-million-iteration stretch.
/// - [`BitLockerError::UnsupportedProtector`] when the volume has no VMK for this
///   protector.
/// - [`BitLockerError::CredentialRejected`] when either AES-CCM tag fails.
/// - [`BitLockerError::MetadataUnreadable`] when required key material is absent
///   or too short.
///
/// `iterations` is crate-internal on purpose. The two public entry points always
/// pass [`STRETCH_ITERATIONS`], so no caller outside this crate can weaken the
/// derivation; tests use the parameter to exercise the orchestration without
/// paying a million SHA-256 rounds per case.
pub(crate) fn derive_key_package(
    metadata: &FveMetadata,
    protector: ProtectorKind,
    protection_code: u16,
    credential_hash: &[u8; 32],
    iterations: u64,
) -> Result<VolumeKeyPackage> {
    let method = metadata.encryption_method;
    let fvek_len = method
        .fvek_len()
        .ok_or(BitLockerError::UnsupportedEncryptionMethod {
            code: metadata.encryption_method_code,
            label: method.label(),
        })?;

    let vmk = metadata
        .vmk_entries()
        .find(|entry| entry.protection_code() == Some(protection_code))
        .ok_or_else(|| BitLockerError::UnsupportedProtector {
            found: describe_inventory(metadata),
        })?;

    // VMK properties are nested entries starting at value-data offset 28.
    let properties = vmk.nested(28);
    let salt = stretch_salt(&properties, protector)?;
    let unwrap_key = stretch_key_n(credential_hash, &salt, iterations);

    let wrapped_vmk = properties
        .iter()
        .find(|entry| entry.value_type == VALUE_TYPE_AES_CCM)
        .ok_or_else(|| BitLockerError::MetadataUnreadable {
            reason: "VMK protector carries no AES-CCM wrapped key".to_string(),
        })?;
    let vmk_container =
        aes_ccm_unwrap(&unwrap_key, &wrapped_vmk.data).ok_or(BitLockerError::CredentialRejected)?;
    let vmk_key = take_key::<32>(&vmk_container, 12, "volume master key")?;

    derive_key_package_from_vmk_bytes(metadata, &vmk_key, fvek_len)
}

pub(crate) fn derive_key_package_from_vmk_bytes(
    metadata: &FveMetadata,
    vmk_key: &[u8; 32],
    fvek_len: usize,
) -> Result<VolumeKeyPackage> {
    let method = metadata.encryption_method;
    let fvek_entry = metadata
        .fvek_entry()
        .ok_or_else(|| BitLockerError::MetadataUnreadable {
            reason: "metadata carries no FVEK entry".to_string(),
        })?;
    let fvek_container =
        aes_ccm_unwrap(vmk_key, &fvek_entry.data).ok_or(BitLockerError::CredentialRejected)?;

    let fvek = take_key_slice(&fvek_container, 12, fvek_len, "FVEK")?;
    let tweak = if method.uses_diffuser_tweak() {
        Some(take_key_slice(&fvek_container, 44, 16, "diffuser tweak")?)
    } else {
        None
    };
    Ok(VolumeKeyPackage::new(fvek, tweak))
}

/// Extracts the stretch salt from a VMK's nested properties.
fn stretch_salt(properties: &[MetadataEntry], protector: ProtectorKind) -> Result<[u8; 16]> {
    let stretch = properties
        .iter()
        .find(|entry| entry.value_type == VALUE_TYPE_STRETCH)
        .ok_or_else(|| BitLockerError::MetadataUnreadable {
            reason: format!("{} protector carries no stretch key", protector.label()),
        })?;
    // The salt sits at stretch value-data offset 4, after the 4-byte method.
    let mut salt = [0u8; 16];
    let source = stretch
        .data
        .get(4..20)
        .ok_or_else(|| BitLockerError::MetadataUnreadable {
            reason: format!("{} stretch key is truncated", protector.label()),
        })?;
    salt.copy_from_slice(source);
    Ok(salt)
}

/// Renders the protector inventory for an unsupported-protector error.
fn describe_inventory(metadata: &FveMetadata) -> String {
    let inventory = metadata.protector_inventory();
    if inventory.is_empty() {
        return "no protectors".to_string();
    }
    inventory
        .protectors()
        .iter()
        .map(|protector| protector.label())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Copies a fixed-size key out of an unwrapped container.
fn take_key<const N: usize>(
    container: &[u8],
    offset: usize,
    what: &str,
) -> Result<Zeroizing<[u8; N]>> {
    let slice = container
        .get(offset..offset.saturating_add(N))
        .ok_or_else(|| BitLockerError::MetadataUnreadable {
            reason: format!(
                "{what} container holds {} bytes, need {}",
                container.len(),
                offset + N
            ),
        })?;
    let mut key = Zeroizing::new([0u8; N]);
    key.copy_from_slice(slice);
    Ok(key)
}

/// Copies a runtime-length key out of an unwrapped container.
fn take_key_slice(container: &[u8], offset: usize, len: usize, what: &str) -> Result<Vec<u8>> {
    container
        .get(offset..offset.saturating_add(len))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| BitLockerError::MetadataUnreadable {
            reason: format!(
                "{what} container holds {} bytes, need {}",
                container.len(),
                offset + len
            ),
        })
}

/// Seeks, mapping the failure onto the evidence-read error.
fn seek_to<R: Seek>(reader: &mut R, offset: u64) -> Result<()> {
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|source| BitLockerError::EvidenceRead { offset, source })?;
    Ok(())
}

/// Fills `buf` completely, treating a short read as a failure.
fn read_exact_at<R: Read + Seek>(reader: &mut R, offset: u64, buf: &mut [u8]) -> Result<()> {
    seek_to(reader, offset)?;
    reader
        .read_exact(buf)
        .map_err(|source| BitLockerError::EvidenceRead { offset, source })
}

#[cfg(test)]
#[path = "../tests/unit/unlock/mod.rs"]
mod tests;
