use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::UNIX_EPOCH;

use super::archive::ArchiveFsReader;

const MAX_ARCHIVE_CACHE_ENTRIES: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ArchiveCacheKey {
    path: PathBuf,
    size: u64,
    modified_nanos: u128,
}

type ArchiveCacheEntry = (ArchiveCacheKey, Arc<ArchiveFsReader>);
type ArchiveCache = VecDeque<ArchiveCacheEntry>;

static ARCHIVE_CACHE: LazyLock<Mutex<ArchiveCache>> = LazyLock::new(|| Mutex::new(VecDeque::new()));

pub(super) fn open_cached(
    path: &Path,
    data_source_name: &str,
) -> std::io::Result<Arc<ArchiveFsReader>> {
    let canonical = path.canonicalize()?;
    let metadata = std::fs::metadata(&canonical)?;
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let key = ArchiveCacheKey {
        path: canonical.clone(),
        size: metadata.len(),
        modified_nanos,
    };
    if let Ok(mut cache) = ARCHIVE_CACHE.lock() {
        if let Some(position) = cache.iter().position(|(cached, _)| *cached == key) {
            let cached = cache
                .remove(position)
                .map(|(_, reader)| reader)
                .ok_or_else(|| invalid_data("archive cache entry disappeared"))?;
            cache.push_front((key, Arc::clone(&cached)));
            return Ok(cached);
        }
    }

    let reader = Arc::new(ArchiveFsReader::open(&canonical, data_source_name)?);
    if let Ok(mut cache) = ARCHIVE_CACHE.lock() {
        cache.retain(|(cached, _)| *cached != key);
        cache.push_front((key, Arc::clone(&reader)));
        cache.truncate(MAX_ARCHIVE_CACHE_ENTRIES);
    }
    Ok(reader)
}

fn invalid_data(message: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}
