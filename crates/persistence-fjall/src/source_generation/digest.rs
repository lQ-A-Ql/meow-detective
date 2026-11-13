use super::SEALED_KEYSPACES;
use crate::{
    source_store::{ensure_not_cancelled, read_bounded_page},
    Result, SnapshotEntry, SourceStore,
};
use sha2::{Digest, Sha256};
use std::sync::atomic::AtomicBool;

pub(super) fn immutable_data_digest(
    source: &SourceStore,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(u64, u64),
) -> Result<String> {
    let snapshot = source.database().read_snapshot();
    let mut hasher = Sha256::new();
    let mut records = 0u64;
    let mut bytes = 0u64;
    for keyspace in SEALED_KEYSPACES {
        ensure_not_cancelled(cancel)?;
        let name = keyspace.as_str().as_bytes();
        hasher.update((name.len() as u64).to_be_bytes());
        hasher.update(name);
        let mut cursor = None;
        loop {
            let (page, page_bytes) =
                read_bounded_page(&snapshot, keyspace, cursor.as_deref(), cancel)?;
            if page.is_empty() {
                break;
            }
            for entry in &page {
                hash_entry(&mut hasher, entry, cancel)?;
            }
            records += page.len() as u64;
            bytes += page_bytes;
            progress(records, bytes);
            ensure_not_cancelled(cancel)?;
            cursor = page.last().map(|entry| entry.key.clone());
        }
    }
    Ok(hex::encode(hasher.finalize()))
}

pub(super) fn hash_entry(
    hasher: &mut Sha256,
    entry: &SnapshotEntry,
    cancel: &AtomicBool,
) -> Result<()> {
    hasher.update((entry.key.len() as u64).to_be_bytes());
    hash_bytes(hasher, &entry.key, cancel)?;
    hasher.update((entry.value.len() as u64).to_be_bytes());
    hash_bytes(hasher, &entry.value, cancel)
}

fn hash_bytes(hasher: &mut Sha256, bytes: &[u8], cancel: &AtomicBool) -> Result<()> {
    for chunk in bytes.chunks(64 * 1024) {
        ensure_not_cancelled(cancel)?;
        hasher.update(chunk);
    }
    Ok(())
}
