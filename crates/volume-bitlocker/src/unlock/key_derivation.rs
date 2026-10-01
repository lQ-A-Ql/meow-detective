//! Authenticated VMK/FVEK key-package derivation from one metadata copy.
use crate::kdf::aes_ccm_unwrap;
use crate::metadata::{VALUE_TYPE_AES_CCM, VALUE_TYPE_STRETCH};
use crate::secret::VolumeKeyPackage;
use crate::{BitLockerError, FveMetadata, MetadataEntry, ProtectorKind, Result};
use zeroize::Zeroizing;
pub(crate) fn checked_fvek_len(metadata: &FveMetadata) -> Result<usize> {
    metadata
        .encryption_method
        .fvek_len()
        .ok_or(BitLockerError::UnsupportedEncryptionMethod {
            code: metadata.encryption_method_code,
            label: metadata.encryption_method.label(),
        })
}

/// Derives the password-protector unwrap key inputs without authenticating the
/// wrapped VMK. Dictionary callers use the salt as a cache key so redundant
/// metadata copies do not repeat the million-round stretch.
pub(crate) fn stretch_salt_for_protector(
    metadata: &FveMetadata,
    protector: ProtectorKind,
    protection_code: u16,
) -> Result<[u8; 16]> {
    let vmk = metadata
        .vmk_entries()
        .find(|entry| entry.protection_code() == Some(protection_code))
        .ok_or_else(|| BitLockerError::UnsupportedProtector {
            found: describe_inventory(metadata),
        })?;

    // VMK properties are nested entries starting at value-data offset 28.
    let properties = vmk.nested(28);
    stretch_salt(&properties, protector)
}

/// Finishes a password attempt with an already stretched VMK unwrap key.
/// Authentication and FVEK validation still run for every metadata copy.
pub(crate) fn derive_key_package_with_unwrap_key(
    metadata: &FveMetadata,
    protection_code: u16,
    unwrap_key: &[u8; 32],
) -> Result<VolumeKeyPackage> {
    let fvek_len = checked_fvek_len(metadata)?;

    let vmk = metadata
        .vmk_entries()
        .find(|entry| entry.protection_code() == Some(protection_code))
        .ok_or_else(|| BitLockerError::UnsupportedProtector {
            found: describe_inventory(metadata),
        })?;
    let properties = vmk.nested(28);

    let wrapped_vmk = properties
        .iter()
        .find(|entry| entry.value_type == VALUE_TYPE_AES_CCM)
        .ok_or_else(|| BitLockerError::MetadataUnreadable {
            reason: "VMK protector carries no AES-CCM wrapped key".to_string(),
        })?;
    let vmk_container =
        aes_ccm_unwrap(unwrap_key, &wrapped_vmk.data).ok_or(BitLockerError::CredentialRejected)?;
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
