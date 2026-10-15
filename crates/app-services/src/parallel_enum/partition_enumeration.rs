use super::batch_sink::{enumerate_fs_to_staging, EnumerationStats};
use super::error::ParallelEnumError;
use super::ntfs::{enumerate_ntfs_mft_to_staging, mft_scan::open_partition_evidence_reader};
use super::{PartitionEnumStrategy, PartitionWork};
use crate::import_pipeline::profile::{Counters, PhaseRecorder};
use crate::staging;
use evidence_core::FileSystemReader;
use persistence_sqlite::repositories::staging_repo::StagingRepo;
use std::sync::atomic::{AtomicBool, Ordering};
use transport::dto::import_benchmark::{
    ImportBenchmarkPhaseDto as Phase, ImportBenchmarkStateDto as State,
};

pub(super) fn enumerate_partition(
    conn: &rusqlite::Connection,
    partition: &PartitionWork,
    data_source_id: &str,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>,
    warnings: &mut Vec<String>,
    profile: &mut PhaseRecorder,
) -> Result<EnumerationStats, ParallelEnumError> {
    if let PartitionEnumStrategy::NtfsDeferred(plan) = &partition.strategy {
        plan.validate(partition)?;
    }
    if !partition.fs_kind.eq_ignore_ascii_case("ntfs") {
        return enumerate_recursive(
            conn,
            partition,
            data_source_id,
            cancel_token,
            progress_cb,
            profile,
        );
    }
    match enumerate_ntfs_mft_to_staging(
        conn,
        partition,
        data_source_id,
        cancel_token,
        progress_cb,
        profile,
    ) {
        Ok(stats) => Ok(stats),
        Err(error) if error.allows_fallback() && !cancel_token.load(Ordering::Relaxed) => {
            tracing::warn!(partition_index = partition.index, %error, "MFT fast path failed; falling back to recursive enum");
            StagingRepo::reset_enum_for_fallback(conn)?;
            let message = error.to_string();
            staging::set_staging_meta(conn, "mft_fallback_warning", &message)?;
            warnings.push(format!("MFT fast path fallback: {message}"));
            let started = profile.start();
            let result = enumerate_recursive(
                conn,
                partition,
                data_source_id,
                cancel_token,
                progress_cb,
                profile,
            );
            profile.record(
                Phase::FallbackEnumeration,
                started,
                if result.is_ok() {
                    State::Completed
                } else if matches!(&result, Err(ParallelEnumError::Cancelled)) {
                    State::Cancelled
                } else {
                    State::Failed
                },
            );
            result
        }
        Err(error) => Err(error),
    }
}

fn enumerate_recursive(
    conn: &rusqlite::Connection,
    partition: &PartitionWork,
    data_source_id: &str,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>,
    profile: &mut PhaseRecorder,
) -> Result<EnumerationStats, ParallelEnumError> {
    if cancel_token.load(Ordering::Relaxed) {
        return Err(ParallelEnumError::Cancelled);
    }
    let run = |filesystem: &dyn FileSystemReader| {
        enumerate_fs_to_staging(
            conn,
            filesystem,
            data_source_id,
            partition.index,
            cancel_token,
            progress_cb,
        )
        .map_err(|message| {
            ParallelEnumError::recursive(message, cancel_token.load(Ordering::Relaxed))
        })
    };
    match &partition.strategy {
        PartitionEnumStrategy::Generic(filesystem) => run(filesystem.as_ref()),
        PartitionEnumStrategy::NtfsDeferred(_) => {
            let filesystem = profile.measure(Phase::FallbackReaderBuild, || {
                let reader = open_partition_evidence_reader(partition)?;
                fs_ntfs::NtfsReader::open(reader, partition.volume_offset)
                    .map_err(ParallelEnumError::Io)
            })?;
            profile.count(
                Phase::FallbackReaderBuild,
                Counters {
                    builds: 1,
                    opens: 1,
                    ..Default::default()
                },
            );
            run(&filesystem)
        }
    }
}
