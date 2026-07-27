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

use zeroize::Zeroizing;

use crate::error::{BitLockerError, Result};
use crate::header::VolumeHeader;
use crate::kdf::{
    aes_ccm_unwrap, password_hash, recovery_key_hash, stretch_key_n, STRETCH_ITERATIONS,
};
use crate::metadata::{
    FveMetadata, MetadataEntry, PROTECTION_PASSWORD, PROTECTION_RECOVERY, VALUE_TYPE_AES_CCM,
    VALUE_TYPE_STRETCH,
};
use crate::protector::ProtectorKind;
use crate::secret::{Passphrase, VolumeKeyPackage};

/// How much of a metadata block to read before parsing it.
///
/// The FVE metadata region is small; 64 KiB covers the header, all entries, and
/// slack without letting a lying size field pull an unbounded read into memory.
const METADATA_READ_LEN: usize = 64 * 1024;

/// The 512-byte volume header sector.
const HEADER_LEN: usize = 512;

/// What a locked volume reveals without any credential.
#[derive(Debug, Clone)]
pub struct VolumeIdentity {
    /// The parsed metadata block.
    pub metadata: FveMetadata,
    /// Bytes per sector, from the volume header.
    pub bytes_per_sector: u16,
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
    let mut header_sector = [0u8; HEADER_LEN];
    seek_to(reader, 0)?;
    read_exact_at(reader, 0, &mut header_sector)?;
    let header = VolumeHeader::parse(&header_sector)?;

    let offsets = header.fve_metadata_offsets;
    for &offset in &offsets {
        if offset == 0 {
            continue;
        }
        let mut block = vec![0u8; METADATA_READ_LEN];
        seek_to(reader, offset)?;
        let read = read_available(reader, offset, &mut block)?;
        block.truncate(read);
        if let Some(metadata) = FveMetadata::parse(&block, header.bytes_per_sector) {
            return Ok(VolumeIdentity {
                metadata,
                bytes_per_sector: header.bytes_per_sector,
            });
        }
    }

    Err(BitLockerError::MetadataUnreadable {
        reason: format!(
            "no -FVE-FS- metadata block at any of the {} candidate offsets {offsets:?}",
            offsets.iter().filter(|offset| **offset != 0).count()
        ),
    })
}

/// Derives the verified key package for a volume using a password.
///
/// # Errors
///
/// See [`derive_key_package`].
pub fn unlock_with_password(
    metadata: &FveMetadata,
    password: &Passphrase,
) -> Result<VolumeKeyPackage> {
    let hash = password_hash(password.expose_for_derivation());
    derive_key_package(
        metadata,
        ProtectorKind::Password,
        PROTECTION_PASSWORD,
        &hash,
        STRETCH_ITERATIONS,
    )
}

/// Derives the verified key package for a volume using a 48-digit recovery password.
///
/// # Errors
///
/// [`BitLockerError::CredentialRejected`] when the recovery password is
/// structurally invalid, plus everything [`derive_key_package`] can return.
/// A malformed recovery password maps to the same rejection as a wrong one so the
/// distinction never reaches a caller that might report it.
pub fn unlock_with_recovery_password(
    metadata: &FveMetadata,
    recovery: &Passphrase,
) -> Result<VolumeKeyPackage> {
    let hash = recovery_key_hash(recovery.expose_for_derivation())
        .map_err(|_| BitLockerError::CredentialRejected)?;
    derive_key_package(
        metadata,
        ProtectorKind::RecoveryPassword,
        PROTECTION_RECOVERY,
        &hash,
        STRETCH_ITERATIONS,
    )
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

    let fvek_entry = metadata
        .fvek_entry()
        .ok_or_else(|| BitLockerError::MetadataUnreadable {
            reason: "metadata carries no FVEK entry".to_string(),
        })?;
    let fvek_container =
        aes_ccm_unwrap(&vmk_key, &fvek_entry.data).ok_or(BitLockerError::CredentialRejected)?;

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
fn read_exact_at<R: Read>(reader: &mut R, offset: u64, buf: &mut [u8]) -> Result<()> {
    reader
        .read_exact(buf)
        .map_err(|source| BitLockerError::EvidenceRead { offset, source })
}

/// Reads as much as is available, returning the byte count.
///
/// A metadata block near the end of a volume can legitimately be short, so this
/// tolerates a partial read and lets the parser judge the result.
fn read_available<R: Read>(reader: &mut R, offset: u64, buf: &mut [u8]) -> Result<usize> {
    let mut filled = 0usize;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(source) => return Err(BitLockerError::EvidenceRead { offset, source }),
        }
    }
    Ok(filled)
}

#[cfg(test)]
#[path = "../tests/unit/unlock/mod.rs"]
mod tests;
