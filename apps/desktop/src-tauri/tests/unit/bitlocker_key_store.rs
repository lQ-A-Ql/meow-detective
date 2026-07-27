#![cfg(windows)]

use app_services::bitlocker_service::BitLockerKeyStore;
use volume_bitlocker::{EncryptionMethod, FveMetadata, MetadataFingerprint};

use super::platform::WindowsCredentialBitLockerKeyStore;
use volume_bitlocker::PersistedKeyBlob;

struct Cleanup(MetadataFingerprint);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = WindowsCredentialBitLockerKeyStore.delete(&self.0);
    }
}

fn unique_fingerprint() -> MetadataFingerprint {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let mut guid = [0u8; 16];
    guid.copy_from_slice(&nonce.to_le_bytes());
    MetadataFingerprint::from_metadata(&FveMetadata {
        encryption_method: EncryptionMethod::XtsAes128,
        encryption_method_code: 0x8004,
        volume_guid: guid,
        creation_time: u64::from(std::process::id()),
        entries: Vec::new(),
        encrypted_volume_size: 0,
        volume_header_offset: 0,
        volume_header_size: 0,
        metadata_offsets: [0; 3],
        metadata_size: 0,
    })
}

#[test]
fn credential_manager_round_trips_and_deletes_a_bounded_blob() {
    let store = WindowsCredentialBitLockerKeyStore;
    let fingerprint = unique_fingerprint();
    let _cleanup = Cleanup(fingerprint.clone());
    let _ = store.delete(&fingerprint).expect("initial cleanup");
    let expected = vec![0x5a; 48];
    let blob = PersistedKeyBlob::from_storage(expected.clone()).expect("bounded blob");

    store.store(&fingerprint, blob).expect("credential write");
    let loaded = store
        .load(&fingerprint)
        .expect("credential read")
        .expect("credential exists");
    assert_eq!(loaded.expose_for_storage(), expected);
    assert!(store.delete(&fingerprint).expect("credential delete"));
    assert!(store.load(&fingerprint).expect("not-found read").is_none());
    assert!(!store.delete(&fingerprint).expect("idempotent delete"));
}
