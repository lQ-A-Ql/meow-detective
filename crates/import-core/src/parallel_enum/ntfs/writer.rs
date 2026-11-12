//! Writer batches are independent of reader chunks and never commit a partition.
use super::super::error::ParallelEnumError;
use super::path_reconstruction::MftCatalog;
use fs_ntfs::mft_scanner::MftRecord;
use rusqlite::CachedStatement;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) const MFT_WRITER_BATCH_RECORDS: usize = 10_000;
#[derive(Default)]
pub(in crate::parallel_enum) struct WriterStats {
    pub rows: u64,
    pub batches: u64,
    pub maximum_records: u64,
}

pub(in crate::parallel_enum) fn stage_batches(
    catalog: &mut MftCatalog,
    statement: &mut CachedStatement<'_>,
    records: &[MftRecord],
    batch_records: usize,
    cancel: &AtomicBool,
) -> Result<WriterStats, ParallelEnumError> {
    if batch_records == 0 {
        return Err(ParallelEnumError::MftParams(
            "zero writer batch size".into(),
        ));
    }
    let mut stats = WriterStats::default();
    for batch in records.chunks(batch_records) {
        if cancel.load(Ordering::Relaxed) {
            return Err(ParallelEnumError::Cancelled);
        }
        stats.rows += catalog.stage_records(statement, batch)?;
        stats.batches += 1;
        stats.maximum_records = stats.maximum_records.max(batch.len() as u64);
    }
    Ok(stats)
}
