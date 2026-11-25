use super::{
    browse_counts, browse_keys, decode_record, CatalogRecordStore, CatalogSort, FileEntryRecord,
};
use crate::{
    FjallStorageError, KeyspaceName, Result, ScanDirection, SnapshotEntry, SourceReadSnapshot,
    VersionedEnvelope,
};

pub struct CatalogBrowseQuery<'a> {
    pub source: &'a str,
    pub parent: Option<&'a str>,
    pub directories_only: bool,
    pub show_hidden: bool,
    pub sort: CatalogSort,
    pub descending: bool,
    pub offset: usize,
    pub limit: usize,
}

pub struct CatalogBrowsePage {
    pub records: Vec<FileEntryRecord>,
    pub total: u64,
}

impl CatalogRecordStore {
    pub fn child_directory_count(
        &self,
        snapshot: &SourceReadSnapshot,
        source: &str,
        parent: Option<&str>,
        show_hidden: bool,
    ) -> Result<u64> {
        browse_counts::require(snapshot, source)?;
        (0..4)
            .filter(|bucket| show_hidden || bucket % 2 == 0)
            .try_fold(0, |total, bucket| {
                Ok(total + browse_counts::count(snapshot, source, parent, bucket)?)
            })
    }

    pub fn browse(
        &self,
        snapshot: &SourceReadSnapshot,
        query: &CatalogBrowseQuery<'_>,
    ) -> Result<CatalogBrowsePage> {
        browse_counts::require(snapshot, query.source)?;
        let window = query
            .offset
            .checked_add(query.limit)
            .filter(|window| *window <= 8_192)
            .ok_or_else(|| {
                FjallStorageError::InvalidEnvelope("catalog browse window exceeds 8192".into())
            })?;
        let buckets = (0..if query.directories_only { 4 } else { 8 })
            .filter(|bucket| query.show_hidden || bucket % 2 == 0)
            .collect::<Vec<_>>();
        let total = buckets.iter().try_fold(0u64, |total, bucket| {
            Ok::<_, FjallStorageError>(
                total + browse_counts::count(snapshot, query.source, query.parent, *bucket)?,
            )
        })?;
        if query.limit == 0 || query.offset as u64 >= total {
            return Ok(CatalogBrowsePage {
                records: Vec::new(),
                total,
            });
        }
        let mut selected = Vec::new();
        if query.directories_only {
            for bucket in buckets {
                let prefix =
                    browse_keys::prefix(query.source, query.parent, CatalogSort::Name, bucket)?;
                selected.extend(
                    snapshot
                        .keyset(
                            KeyspaceName::CatalogIndexes,
                            &prefix,
                            None,
                            ScanDirection::Forward,
                            window,
                        )?
                        .into_iter()
                        .map(|entry| (entry.key[prefix.len()..].to_vec(), entry)),
                );
            }
            selected.sort_by(|left, right| left.0.cmp(&right.0));
        } else {
            let mut remaining = window;
            for bucket in buckets {
                if remaining == 0 {
                    break;
                }
                let prefix = browse_keys::prefix(query.source, query.parent, query.sort, bucket)?;
                let entries = self.browse_bucket(snapshot, query, &prefix, remaining)?;
                remaining -= entries.len();
                selected.extend(entries.into_iter().map(|entry| (Vec::new(), entry)));
            }
        }
        let records = selected
            .into_iter()
            .skip(query.offset)
            .take(query.limit)
            .map(|(_, entry)| self.browse_record(snapshot, &entry))
            .collect::<Result<Vec<_>>>()?;
        Ok(CatalogBrowsePage { records, total })
    }

    fn browse_record(
        &self,
        snapshot: &SourceReadSnapshot,
        entry: &SnapshotEntry,
    ) -> Result<FileEntryRecord> {
        let bytes = snapshot
            .get(KeyspaceName::CatalogRecords, &entry.value)?
            .ok_or(FjallStorageError::CatalogIndexCorrupt)?;
        decode_record(VersionedEnvelope::decode(&bytes)?)
    }

    fn browse_bucket(
        &self,
        snapshot: &SourceReadSnapshot,
        query: &CatalogBrowseQuery<'_>,
        prefix: &[u8],
        limit: usize,
    ) -> Result<Vec<SnapshotEntry>> {
        if !query.descending || query.sort == CatalogSort::Name {
            let direction = if query.descending {
                ScanDirection::Reverse
            } else {
                ScanDirection::Forward
            };
            return snapshot.keyset(KeyspaceName::CatalogIndexes, prefix, None, direction, limit);
        }
        let mut result = Vec::new();
        let mut upper = None;
        while result.len() < limit {
            let previous = snapshot.keyset(
                KeyspaceName::CatalogIndexes,
                prefix,
                upper.as_deref(),
                ScanDirection::Reverse,
                1,
            )?;
            let Some(last) = previous.first() else {
                break;
            };
            let record = self.browse_record(snapshot, last)?;
            let mut group = prefix.to_vec();
            group.extend(browse_keys::primary(&record, query.sort)?);
            result.extend(snapshot.keyset(
                KeyspaceName::CatalogIndexes,
                &group,
                None,
                ScanDirection::Forward,
                limit - result.len(),
            )?);
            upper = Some(group);
        }
        Ok(result)
    }
}
