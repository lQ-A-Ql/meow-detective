pub(super) mod mft_scan;
pub(super) mod path_reconstruction;
pub(super) mod size_reconciliation;
pub(super) mod validation;

use super::batch_sink::{prepare_mft_insert, EnumerationStats};
use super::error::ParallelEnumError;
use super::partition_work::PartitionWork;
use crate::staging;
use mft_scan::prepare_mft_scan;
use path_reconstruction::MftCatalog;
use std::sync::atomic::{AtomicBool, Ordering};
use validation::validate_mft_staging_shape;

pub(super) fn enumerate_ntfs_mft_to_staging(
    conn: &rusqlite::Connection,
    partition: &PartitionWork,
    data_source_id: &str,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>,
) -> Result<EnumerationStats, ParallelEnumError> {
    check_cancelled(cancel_token)?;
    let mut scan = prepare_mft_scan(partition)?;
    enumerate_ntfs_mft_scan_to_staging(
        conn,
        &mut scan,
        data_source_id,
        partition.index,
        partition.volume_offset,
        cancel_token,
        progress_cb,
    )
}

pub(super) fn enumerate_ntfs_reader_to_staging(
    conn: &rusqlite::Connection,
    reader: Box<dyn evidence_core::EvidenceReader>,
    data_source_id: &str,
    partition_index: usize,
    volume_offset: u64,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>,
) -> Result<EnumerationStats, ParallelEnumError> {
    check_cancelled(cancel_token)?;
    let mut scan = mft_scan::prepare_mft_scan_from_reader(reader, volume_offset)?;
    enumerate_ntfs_mft_scan_to_staging(
        conn,
        &mut scan,
        data_source_id,
        partition_index,
        volume_offset,
        cancel_token,
        progress_cb,
    )
}

fn enumerate_ntfs_mft_scan_to_staging(
    conn: &rusqlite::Connection,
    scan: &mut mft_scan::MftScan,
    data_source_id: &str,
    partition_index: usize,
    volume_offset: u64,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>,
) -> Result<EnumerationStats, ParallelEnumError> {
    let total_records = scan.total_records();

    conn.execute_batch("BEGIN TRANSACTION")
        .map_err(ParallelEnumError::Db)?;
    let result = scan_mft_transaction(
        conn,
        data_source_id,
        partition_index,
        volume_offset,
        cancel_token,
        progress_cb,
        scan,
    );
    let stats = match result {
        Ok(stats) => {
            conn.execute_batch("COMMIT")
                .map_err(ParallelEnumError::Db)?;
            stats
        }
        Err(error) => {
            conn.execute_batch("ROLLBACK").ok();
            return Err(error);
        }
    };

    staging::set_staging_meta(conn, "enum_strategy", "mft")
        .map_err(|error| ParallelEnumError::MftParams(format!("Mark MFT strategy: {error}")))?;
    staging::set_staging_meta(conn, "mft_records", &total_records.to_string())
        .map_err(|error| ParallelEnumError::MftParams(format!("Mark MFT record count: {error}")))?;
    Ok(stats)
}

fn scan_mft_transaction(
    conn: &rusqlite::Connection,
    data_source_id: &str,
    partition_index: usize,
    volume_offset: u64,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>,
    scan: &mut mft_scan::MftScan,
) -> Result<EnumerationStats, ParallelEnumError> {
    let mut statement = prepare_mft_insert(conn)?;
    let mut catalog = MftCatalog::default();
    let mut start_record = 0;

    while start_record < scan.total_records() {
        check_cancelled(cancel_token)?;
        let (records, scanned_count) = scan.read_chunk(start_record)?;
        catalog.stage_records(&mut statement, &records, data_source_id, partition_index)?;
        start_record += scanned_count;
        if let Some(callback) = progress_cb {
            let stats = catalog.stats();
            callback(stats.file_count + stats.dir_count, stats.total_size);
        }
    }

    drop(statement);
    scan.release_buffer();
    catalog.backfill_directory_indexes(
        conn,
        data_source_id,
        scan.take_reader()?,
        partition_index,
        volume_offset,
    )?;
    catalog.update_staging_paths(conn, data_source_id, partition_index)?;
    let stats = catalog.stats();
    drop(catalog);
    validate_mft_staging_shape(conn, data_source_id, partition_index)?;
    Ok(stats)
}

fn check_cancelled(cancel_token: &AtomicBool) -> Result<(), ParallelEnumError> {
    if cancel_token.load(Ordering::Relaxed) {
        Err(ParallelEnumError::Cancelled)
    } else {
        Ok(())
    }
}
