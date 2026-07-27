use super::*;
use crate::metadata::MetadataEntry;
use crate::method::EncryptionMethod;

/// Builds metadata directly rather than by parsing a synthetic volume.
///
/// The fingerprint is a pure function of the identity fields, so constructing
/// them is both cheaper and more precise than round-tripping bytes: each test
/// varies exactly one input.
fn metadata_with(
    volume_guid: [u8; 16],
    creation_time: u64,
    method_code: u16,
    protector_codes: &[u16],
) -> FveMetadata {
    let entries = protector_codes
        .iter()
        .map(|code| {
            let mut data = vec![0u8; 28];
            data[26..28].copy_from_slice(&code.to_le_bytes());
            MetadataEntry {
                entry_type: 0x0002,
                value_type: 0x0008,
                version: 1,
                data,
            }
        })
        .collect();
    FveMetadata {
        encryption_method: EncryptionMethod::from_code(method_code),
        encryption_method_code: method_code,
        volume_guid,
        creation_time,
        entries,
        encrypted_volume_size: 0,
        volume_header_offset: 0x3000,
        volume_header_size: 512,
        metadata_offsets: [0x1000, 0, 0],
        metadata_size: 128,
    }
}

fn baseline() -> FveMetadata {
    metadata_with([0xAB; 16], 0x01D9_0000_0000_0000, 0x8000, &[0x2000])
}

#[test]
fn fingerprint_is_stable_for_the_same_volume() {
    // Reopening the same evidence must reach the same stored key package.
    assert_eq!(
        MetadataFingerprint::from_metadata(&baseline()),
        MetadataFingerprint::from_metadata(&baseline())
    );
}

#[test]
fn fingerprint_changes_with_the_volume_guid() {
    let other = metadata_with([0xCD; 16], 0x01D9_0000_0000_0000, 0x8000, &[0x2000]);
    assert_ne!(
        MetadataFingerprint::from_metadata(&baseline()),
        MetadataFingerprint::from_metadata(&other)
    );
}

#[test]
fn fingerprint_changes_with_the_creation_time() {
    // Two volumes can share a GUID across a reformat; the creation time is what
    // separates them.
    let other = metadata_with([0xAB; 16], 0x01DA_0000_0000_0000, 0x8000, &[0x2000]);
    assert_ne!(
        MetadataFingerprint::from_metadata(&baseline()),
        MetadataFingerprint::from_metadata(&other)
    );
}

#[test]
fn fingerprint_changes_with_the_encryption_method() {
    let other = metadata_with([0xAB; 16], 0x01D9_0000_0000_0000, 0x8004, &[0x2000]);
    assert_ne!(
        MetadataFingerprint::from_metadata(&baseline()),
        MetadataFingerprint::from_metadata(&other)
    );
}

#[test]
fn fingerprint_changes_when_the_protector_set_changes() {
    // A re-keyed volume must not silently reuse a stored key package.
    let with_recovery = metadata_with([0xAB; 16], 0x01D9_0000_0000_0000, 0x8000, &[0x2000, 0x0800]);
    assert_ne!(
        MetadataFingerprint::from_metadata(&baseline()),
        MetadataFingerprint::from_metadata(&with_recovery)
    );
}

#[test]
fn fingerprint_depends_on_protector_order() {
    // Order is on-disk evidence, so two volumes that list the same protectors in
    // a different order are not treated as the same volume.
    let forward = metadata_with([0xAB; 16], 0x01D9_0000_0000_0000, 0x8000, &[0x2000, 0x0800]);
    let reversed = metadata_with([0xAB; 16], 0x01D9_0000_0000_0000, 0x8000, &[0x0800, 0x2000]);
    assert_ne!(
        MetadataFingerprint::from_metadata(&forward),
        MetadataFingerprint::from_metadata(&reversed)
    );
}

#[test]
fn fingerprint_is_32_lowercase_hex_characters() {
    let fingerprint = MetadataFingerprint::from_metadata(&baseline());
    let text = fingerprint.as_str();
    assert_eq!(text.len(), 32);
    assert!(text
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
}

#[test]
fn credential_target_matches_the_documented_namespace() {
    let fingerprint = MetadataFingerprint::from_metadata(&baseline());
    let target = fingerprint.credential_target();
    assert_eq!(
        target,
        format!("Meow_Detective/BitLocker/v1/{}", fingerprint.as_str())
    );
}
