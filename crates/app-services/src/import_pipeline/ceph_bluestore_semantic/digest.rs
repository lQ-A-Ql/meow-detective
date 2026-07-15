use ceph_wire::{
    BlueStoreAttributeSummary, BlueStoreBlobUseRef, BlueStoreBlobUseTracker, BlueStoreChecksumType,
};
use sha2::{Digest, Sha256};

pub(super) fn attributes_sha256(attributes: &[BlueStoreAttributeSummary]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"meow-detective/ceph-bluestore-attributes/v1\0");
    digest.update((attributes.len() as u64).to_be_bytes());
    for attribute in attributes {
        update_bytes(&mut digest, &attribute.name);
        digest.update(attribute.value_length.to_be_bytes());
        digest.update(attribute.value_sha256);
    }
    hex::encode(digest.finalize())
}

pub(super) fn use_tracker_sha256(tracker: &BlueStoreBlobUseTracker) -> String {
    let mut digest = Sha256::new();
    digest.update(b"meow-detective/ceph-bluestore-use-tracker/v1\0");
    match tracker {
        BlueStoreBlobUseTracker::V1LegacyRefMap { entries } => {
            digest.update([1]);
            digest.update((entries.len() as u64).to_be_bytes());
            for entry in entries {
                write_ref(&mut digest, entry);
            }
        }
        BlueStoreBlobUseTracker::V2 {
            allocation_unit_size,
            declared_allocation_units,
            referenced_bytes,
        } => {
            digest.update([2]);
            digest.update(allocation_unit_size.to_be_bytes());
            digest.update(declared_allocation_units.to_be_bytes());
            digest.update((referenced_bytes.len() as u64).to_be_bytes());
            for value in referenced_bytes {
                digest.update(value.to_be_bytes());
            }
        }
    }
    hex::encode(digest.finalize())
}

pub(super) fn checksum_type_name(value: BlueStoreChecksumType) -> &'static str {
    match value {
        BlueStoreChecksumType::XxHash32 => "xxHash32",
        BlueStoreChecksumType::XxHash64 => "xxHash64",
        BlueStoreChecksumType::Crc32c => "crc32c",
        BlueStoreChecksumType::Crc32c16 => "crc32c16",
        BlueStoreChecksumType::Crc32c8 => "crc32c8",
    }
}

pub(super) fn checksum_word_size(value: BlueStoreChecksumType) -> u64 {
    match value {
        BlueStoreChecksumType::XxHash32 | BlueStoreChecksumType::Crc32c => 4,
        BlueStoreChecksumType::XxHash64 => 8,
        BlueStoreChecksumType::Crc32c16 => 2,
        BlueStoreChecksumType::Crc32c8 => 1,
    }
}

fn write_ref(digest: &mut Sha256, entry: &BlueStoreBlobUseRef) {
    digest.update(entry.offset.to_be_bytes());
    digest.update(entry.length.to_be_bytes());
    digest.update(entry.refs.to_be_bytes());
}

fn update_bytes(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value);
}
