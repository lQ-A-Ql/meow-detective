use super::*;
use crate::{EncryptionMethod, FveMetadata, MetadataEntry};

fn identity(guid: u8) -> VolumeIdentity {
    VolumeIdentity {
        metadata: FveMetadata {
            encryption_method: EncryptionMethod::XtsAes128,
            encryption_method_code: 0x8004,
            volume_guid: [guid; 16],
            creation_time: 42,
            entries: vec![MetadataEntry {
                entry_type: 2,
                value_type: 8,
                version: 1,
                data: vec![0; 28],
            }],
            encrypted_volume_size: 1024 * 1024,
            volume_header_offset: 0,
            volume_header_size: 512,
            metadata_offsets: [4096, 8192, 12288],
            metadata_size: 128,
        },
        bytes_per_sector: 512,
    }
}

fn encoded_bytes(identity: &VolumeIdentity) -> Vec<u8> {
    encode(identity, &VolumeKeyPackage::new(vec![0x5a; 32], None))
        .expose_for_storage()
        .to_vec()
}

fn restore_error(identity: VolumeIdentity, bytes: Vec<u8>) -> BitLockerError {
    let blob = PersistedKeyBlob::from_storage(bytes).expect("bounded envelope");
    match restore(identity, blob) {
        Ok(_) => panic!("expected persisted key restore to fail"),
        Err(error) => error,
    }
}

#[test]
fn v1_envelope_restores_runtime_state_for_the_same_volume() {
    let source = identity(0x42);
    let original = encoded_bytes(&source);
    let blob = PersistedKeyBlob::from_storage(original.clone()).expect("valid blob");
    let restored = restore(source, blob).expect("matching package restores");

    assert_eq!(restored.persisted_key_blob().expose_for_storage(), original);
}

#[test]
fn envelope_rejects_a_different_metadata_fingerprint() {
    let bytes = encoded_bytes(&identity(0x42));
    let error = restore_error(identity(0x24), bytes);

    assert_eq!(error.code(), "BITLOCKER_STORED_KEY_MISMATCH");
}

#[test]
fn envelope_rejects_a_method_mismatch() {
    let source = identity(0x42);
    let mut bytes = encoded_bytes(&source);
    bytes[10..12].copy_from_slice(&0x8005u16.to_le_bytes());

    assert_eq!(
        restore_error(source, bytes).code(),
        "BITLOCKER_STORED_KEY_MISMATCH"
    );
}

#[test]
fn envelope_rejects_invalid_lengths_and_trailing_data() {
    let source = identity(0x42);
    let mut invalid_length = encoded_bytes(&source);
    invalid_length[44..46].copy_from_slice(&16u16.to_le_bytes());
    assert_eq!(
        restore_error(source.clone(), invalid_length).code(),
        "BITLOCKER_STORED_KEY_INVALID"
    );

    let mut trailing = encoded_bytes(&source);
    trailing.push(0);
    assert_eq!(
        restore_error(source, trailing).code(),
        "BITLOCKER_STORED_KEY_INVALID"
    );
}

#[test]
fn storage_blob_rejects_truncated_and_oversized_allocations() {
    for bytes in [vec![0u8; 47], vec![0u8; 129]] {
        let error = match PersistedKeyBlob::from_storage(bytes) {
            Ok(_) => panic!("out-of-bounds blob must fail"),
            Err(error) => error,
        };
        assert_eq!(error.code(), "BITLOCKER_STORED_KEY_INVALID");
    }
}

#[test]
fn envelope_rejects_unknown_version_and_corrupt_magic() {
    let source = identity(0x42);
    let mut version = encoded_bytes(&source);
    version[8..10].copy_from_slice(&2u16.to_le_bytes());
    assert_eq!(
        restore_error(source.clone(), version).code(),
        "BITLOCKER_STORED_KEY_INVALID"
    );

    let mut magic = encoded_bytes(&source);
    magic[0] ^= 0xff;
    assert_eq!(
        restore_error(source, magic).code(),
        "BITLOCKER_STORED_KEY_INVALID"
    );
}
