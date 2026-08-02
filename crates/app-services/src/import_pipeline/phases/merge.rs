use std::time::Instant;

use transport::CommandError;

use crate::{file_service, staging};

use crate::import_pipeline::context::ImportJobContext;
use crate::import_pipeline::profile::{elapsed_ms, emit_phase_profile, rows_per_sec};

pub(super) fn merge_enumeration_results(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
    manifest: &staging::StagingManifest,
) -> Result<file_service::EnumerationStats, CommandError> {
    crate::import_pipeline::emit::emit_job_progress(
        ctx.event_sink(),
        &ctx.job_id.0,
        62,
        "Merging partitions...",
    );
    persist_merging_phase(ctx.case_root, manifest)?;

    let started = Instant::now();
    let merged = staging::merge_all_staging_to_main(
        ctx.source_connection()?,
        ctx.case_root,
        &data_source.id.0,
        manifest,
        Some(&|completed, total| {
            let percent = 62 + (completed as u32 * 8 / total as u32);
            crate::import_pipeline::emit::emit_job_progress(
                ctx.event_sink(),
                &ctx.job_id.0,
                percent.min(70),
                &format!("Merged {completed}/{total} partitions"),
            );
        }),
    )
    .map_err(CommandError::from_service_error)?;
    report_merge_complete(ctx, data_source, merged, started.elapsed());

    Ok(file_service::EnumerationStats {
        file_count: manifest
            .partitions
            .iter()
            .map(|partition| partition.file_count)
            .sum(),
        dir_count: manifest
            .partitions
            .iter()
            .map(|partition| partition.dir_count)
            .sum(),
        total_size: manifest
            .partitions
            .iter()
            .map(|partition| partition.total_size)
            .sum(),
        warnings: collect_partition_warnings(ctx.case_root, &data_source.id.0, manifest),
        diagnostics: Vec::new(),
    })
}

fn persist_merging_phase(
    case_root: &std::path::Path,
    manifest: &staging::StagingManifest,
) -> Result<(), CommandError> {
    let mut updated = manifest.clone();
    updated.phase = staging::ImportPhase::Merging;
    updated
        .save(case_root)
        .map_err(CommandError::from_service_error)
}

fn report_merge_complete(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
    merged: u64,
    elapsed: std::time::Duration,
) {
    emit_phase_profile(
        ctx.event_sink(),
        ctx.job_id,
        ctx.case_id,
        Some(&data_source.id),
        70,
        format!(
            "Partition merge complete: phase=enum-merge elapsedMs={} rows={} rowsPerSec={} rssMb={}",
            elapsed_ms(elapsed),
            merged,
            rows_per_sec(merged, elapsed),
            crate::runtime_resources::current_rss_mb()
        ),
        ctx.cancel_requested(),
    );
}

fn collect_partition_warnings(
    case_root: &std::path::Path,
    data_source_id: &str,
    manifest: &staging::StagingManifest,
) -> Vec<String> {
    manifest
        .partitions
        .iter()
        .flat_map(|partition| partition_warnings(case_root, data_source_id, partition))
        .collect()
}

fn partition_warnings(
    case_root: &std::path::Path,
    data_source_id: &str,
    partition: &staging::PartitionEntry,
) -> Vec<String> {
    if let Some(error) = &partition.error {
        return vec![format!("Partition {}: {error}", partition.index)];
    }
    let path = staging::staging_db_path(case_root, data_source_id, partition.index);
    if !path.exists() {
        return Vec::new();
    }
    let Ok(connection) =
        staging::open_partition_staging(case_root, data_source_id, partition.index)
    else {
        return Vec::new();
    };
    let Ok(Some(warnings)) = staging::get_staging_meta(&connection, "warnings") else {
        return Vec::new();
    };
    warnings
        .lines()
        .filter(|warning| !warning.is_empty())
        .map(|warning| format!("Partition {}: {warning}", partition.index))
        .collect()
}
