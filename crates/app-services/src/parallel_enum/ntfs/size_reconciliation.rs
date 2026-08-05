use super::super::batch_sink::EnumerationStats;
use super::super::error::ParallelEnumError;
use fs_ntfs::mft_scanner::MftRecord;
use fs_ntfs::NtfsReader;
use rusqlite::{params, Connection};
use std::collections::HashMap;

#[derive(Default)]
pub(in crate::parallel_enum) struct ExternalFileSizes {
    candidates: HashMap<u64, u64>,
}

impl ExternalFileSizes {
    pub(in crate::parallel_enum) fn track(&mut self, record: &MftRecord) {
        if record.has_attribute_list && !record.is_dir {
            self.candidates
                .entry(record.record_number)
                .or_insert(record.size);
        }
    }

    pub(in crate::parallel_enum) fn reconcile(
        &self,
        conn: &Connection,
        ntfs: &NtfsReader,
        data_source_id: &str,
        partition_index: usize,
        stats: &mut EnumerationStats,
    ) -> Result<(), ParallelEnumError> {
        let mut candidates = self.candidates.iter().collect::<Vec<_>>();
        candidates.sort_by_key(|(inode, _)| **inode);
        for (&inode, &catalog_size) in candidates {
            let authoritative_size = match ntfs.file_size_by_inode(inode) {
                Ok(Some(size)) => size,
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!(
                        inode,
                        %error,
                        "Failed to resolve NTFS external $DATA size; retaining catalog size"
                    );
                    continue;
                }
            };
            apply_authoritative_size(
                conn,
                data_source_id,
                partition_index,
                inode,
                catalog_size,
                authoritative_size,
                stats,
            )?;
        }
        Ok(())
    }
}

pub(in crate::parallel_enum) fn apply_authoritative_size(
    conn: &Connection,
    data_source_id: &str,
    partition_index: usize,
    inode: u64,
    catalog_size: u64,
    authoritative_size: u64,
    stats: &mut EnumerationStats,
) -> rusqlite::Result<bool> {
    if catalog_size == authoritative_size {
        return Ok(false);
    }
    let id = format!("mft:{partition_index}:{inode}");
    let changed = conn.execute(
        "UPDATE file_entries SET size = ?1 WHERE id = ?2 AND data_source_id = ?3",
        params![authoritative_size, id, data_source_id],
    )?;
    if changed > 0 {
        stats.total_size = stats
            .total_size
            .saturating_sub(catalog_size)
            .saturating_add(authoritative_size);
    }
    Ok(changed > 0)
}
