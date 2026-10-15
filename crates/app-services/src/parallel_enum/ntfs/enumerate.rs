use super::super::batch_sink::{prepare_mft_insert, EnumerationStats};
use super::super::error::ParallelEnumError;
use super::super::partition_work::PartitionWork;
use super::mft_scan::{self, prepare_mft_scan};
use super::parallel_scan::collect_batches;
use super::path_reconstruction::MftCatalog;
use super::validation::validate_mft_staging_shape;
use crate::import_pipeline::profile::{Counters, PhaseRecorder};
use crate::staging;
use std::sync::atomic::{AtomicBool, Ordering};
use transport::dto::import_benchmark::ImportBenchmarkPhaseDto as Phase;

pub(in crate::parallel_enum) fn enumerate_ntfs_mft_to_staging(
    conn: &rusqlite::Connection,
    partition: &PartitionWork,
    data_source_id: &str,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>,
    profile: &mut PhaseRecorder,
) -> Result<EnumerationStats, ParallelEnumError> {
    check_cancelled(cancel_token)?;
    let mut scan = prepare_mft_scan(partition, profile)?;
    enumerate_scan(
        &mut EnumerationContext {
            conn,
            data_source_id,
            partition_index: partition.index,
            volume_offset: partition.volume_offset,
            factory: partition.reader_factory.as_deref(),
            workers: partition.mft_workers,
            cancel_token,
            progress_cb,
            profile,
        },
        &mut scan,
    )
}

pub(in crate::parallel_enum) fn enumerate_ntfs_reader_to_staging(
    conn: &rusqlite::Connection,
    reader: Box<dyn evidence_core::EvidenceReader>,
    data_source_id: &str,
    partition_index: usize,
    volume_offset: u64,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>,
) -> Result<EnumerationStats, ParallelEnumError> {
    check_cancelled(cancel_token)?;
    let mut profile = PhaseRecorder::new(None, Some(partition_index));
    let mut scan = mft_scan::prepare_mft_scan_from_reader(reader, volume_offset, &mut profile)?;
    enumerate_scan(
        &mut EnumerationContext {
            conn,
            data_source_id,
            partition_index,
            volume_offset,
            factory: None,
            workers: 1,
            cancel_token,
            progress_cb,
            profile: &mut profile,
        },
        &mut scan,
    )
}

struct EnumerationContext<'a> {
    conn: &'a rusqlite::Connection,
    data_source_id: &'a str,
    partition_index: usize,
    volume_offset: u64,
    factory: Option<&'a dyn evidence_core::EvidenceReaderFactory>,
    workers: usize,
    cancel_token: &'a AtomicBool,
    progress_cb: Option<&'a dyn Fn(u64, u64)>,
    profile: &'a mut PhaseRecorder,
}

fn enumerate_scan(
    ctx: &mut EnumerationContext<'_>,
    scan: &mut mft_scan::MftScan,
) -> Result<EnumerationStats, ParallelEnumError> {
    let total_records = scan.total_records();
    ctx.conn
        .execute_batch("BEGIN TRANSACTION")
        .map_err(ParallelEnumError::Db)?;
    let result = (|| {
        persistence_sqlite::repositories::ntfs_staging_repo::NtfsStagingRepo::reset_layout(
            ctx.conn,
        )?;
        ctx.conn.execute_batch("DELETE FROM file_entries")?;
        persistence_sqlite::repositories::ntfs_staging_repo::NtfsStagingRepo::begin(
            ctx.conn,
            ctx.data_source_id,
            ctx.partition_index,
        )?;
        scan_mft_transaction(ctx, scan)
    })();
    let stats = match result {
        Ok(stats) => {
            let commit = ctx
                .profile
                .measure(Phase::MftCommit, || ctx.conn.execute_batch("COMMIT"))
                .map_err(ParallelEnumError::Db);
            if let Err(error) = commit {
                ctx.conn.execute_batch("ROLLBACK").ok();
                return Err(error);
            }
            stats
        }
        Err(error) => {
            ctx.conn.execute_batch("ROLLBACK").ok();
            return Err(error);
        }
    };
    staging::set_staging_meta(ctx.conn, "enum_strategy", "mft")?;
    staging::set_staging_meta(ctx.conn, "mft_records", &total_records.to_string())?;
    Ok(stats)
}

fn scan_mft_transaction(
    ctx: &mut EnumerationContext<'_>,
    scan: &mut mft_scan::MftScan,
) -> Result<EnumerationStats, ParallelEnumError> {
    let mut statement = prepare_mft_insert(ctx.conn)?;
    let mut catalog = MftCatalog::default();
    let scan_record_bytes = scan.record_size() as u64;
    collect_batches(
        scan,
        ctx.factory,
        ctx.workers,
        ctx.cancel_token,
        ctx.profile,
        |batch, profile| {
            if batch.logical_bytes != batch.scanned_records * scan_record_bytes {
                return Err(ParallelEnumError::MftParams(
                    "invalid MFT logical byte count".into(),
                ));
            }
            let staged_rows = profile.measure(Phase::MftStage, || {
                catalog.stage_records(
                    &mut statement,
                    &batch.records,
                    ctx.data_source_id,
                    ctx.partition_index,
                )
            })?;
            profile.count(
                Phase::MftStage,
                Counters {
                    staged: staged_rows,
                    ..Default::default()
                },
            );
            if let Some(callback) = ctx.progress_cb {
                let stats = catalog.stats();
                callback(stats.file_count + stats.dir_count, stats.total_size);
            }
            Ok(())
        },
    )?;
    drop(statement);
    scan.release_buffer();
    catalog.backfill_directory_indexes(
        ctx.conn,
        ctx.data_source_id,
        scan.take_reader()?,
        ctx.partition_index,
        ctx.volume_offset,
        ctx.profile,
    )?;
    ctx.profile.measure(Phase::MftPathResolve, || {
        catalog.update_staging_paths(ctx.conn, ctx.data_source_id, ctx.partition_index)
    })?;
    let stats = catalog.stats();
    drop(catalog);
    ctx.profile.measure(Phase::MftValidate, || {
        validate_mft_staging_shape(ctx.conn, ctx.data_source_id, ctx.partition_index)
    })?;
    Ok(stats)
}

fn check_cancelled(cancel_token: &AtomicBool) -> Result<(), ParallelEnumError> {
    if cancel_token.load(Ordering::Relaxed) {
        Err(ParallelEnumError::Cancelled)
    } else {
        Ok(())
    }
}
