use std::sync::Arc;
use std::time::Instant;

use evidence_core::LogicalFsReader;
use transport::CommandError;

use crate::{datasource_service, file_service, staging};

use super::{merge, probe};
use crate::import_pipeline::context::ImportJobContext;
use crate::import_pipeline::execute::{emit_import_cancellation_state, mark_import_cancelling};
use crate::import_pipeline::partition::build_partition_work;
use crate::import_pipeline::profile::{
    bytes_to_mb, elapsed_ms, emit_import_profile_progress, emit_phase_profile, mb_per_sec,
    rows_per_sec,
};

type PartitionWorkQueue = Vec<crate::parallel_enum::PartitionWork>;
type PartitionBuildFailures = Vec<(usize, String)>;

pub(crate) fn run_enumeration_phase(
    ctx: &mut ImportJobContext<'_>,
    data_source: &domain::DataSource,
) -> Result<file_service::EnumerationStats, CommandError> {
    ctx.report_job_progress(25, "Enumerating filesystem...")?;
    let mut stats = match ctx.import_config.kind {
        domain::DataSourceKind::LogicalDirectory => enumerate_logical_directory(ctx, data_source)
            .map_err(CommandError::from_service_error)?,
        domain::DataSourceKind::E01
        | domain::DataSourceKind::Raw
        | domain::DataSourceKind::LocalDisk => {
            enumerate_image_data_source_with_staging(ctx, data_source)?
        }
        domain::DataSourceKind::CephRbd | domain::DataSourceKind::CephFs => {
            return Err(CommandError::unsupported(
                "Ceph RBD derived sources do not use the ordinary import pipeline",
            ))
        }
    };
    populate_file_graph(ctx, data_source, &mut stats);
    ctx.counts.add_warnings(stats.warnings.len());
    report_catalog_ready(ctx, data_source, &stats);
    reject_cancelled_before_analysis(ctx, data_source)?;
    Ok(stats)
}
fn enumerate_logical_directory(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
) -> Result<file_service::EnumerationStats, persistence_sqlite::DbError> {
    let fs = LogicalFsReader::open(&ctx.import_config.source_path, &data_source.name)?;
    let source_conn = ctx.source_conn.ok_or_else(|| {
        persistence_sqlite::DbError::System("source DB connection is not initialized".to_string())
    })?;
    file_service::enumerate_filesystem_with_root_name_and_cancel(
        source_conn,
        &data_source.id,
        &fs,
        None,
        None::<&dyn Fn(u32)>,
        Some(ctx.options.cancel_token),
    )
}
fn enumerate_image_data_source_with_staging(
    ctx: &mut ImportJobContext<'_>,
    data_source: &domain::DataSource,
) -> Result<file_service::EnumerationStats, CommandError> {
    let mut manifest = staging::StagingManifest::load(ctx.case_root, &data_source.id.0)
        .unwrap_or_else(|| {
            staging::StagingManifest::create(
                &data_source.id.0,
                ctx.source_path,
                ctx.import_config.staging_kind().unwrap_or("Raw"),
            )
        });
    let mut candidates = Vec::new();
    let seed_outcome =
        probe::seed_manifest_if_needed(ctx, data_source, &mut manifest, &mut candidates)?;
    if seed_outcome == probe::ProbeSeedOutcome::CephBlueStoreMetadata {
        ctx.content_kind =
            crate::import_pipeline::context::ImportContentKind::CephBlueStoreMetadata;
        staging::cleanup_staging(ctx.case_root, &data_source.id.0);
        return Ok(file_service::EnumerationStats {
            file_count: 0,
            dir_count: 0,
            total_size: 0,
            warnings: vec![
                "Ceph BlueStore metadata was inventoried; filesystem enumeration is not applicable"
                    .to_string(),
            ],
            diagnostics: Vec::new(),
        });
    }
    if manifest.partitions.is_empty() {
        return Err(CommandError::internal(
            "No supported filesystem partitions were detected",
        ));
    }
    probe::refresh_partition_statuses(ctx.case_root, &data_source.id.0, &mut manifest)?;
    crate::import_pipeline::emit::emit_job_progress(
        ctx.event_sink(),
        &ctx.job_id.0,
        30,
        "Building filesystem readers...",
    );
    probe::load_resume_candidates(ctx, data_source, &mut candidates)?;

    let (pending, failures) =
        build_pending_partition_work(ctx, data_source, &candidates, &mut manifest)?;
    if pending.is_empty() {
        return handle_no_pending_partitions(ctx, &manifest, failures);
    }
    enumerate_pending_partitions(ctx, data_source, &mut manifest, pending)?;
    reject_cancelled_after_enumeration(ctx)?;
    merge::merge_enumeration_results(ctx, data_source, &manifest)
}

fn build_pending_partition_work(
    ctx: &mut ImportJobContext<'_>,
    data_source: &domain::DataSource,
    candidates: &[datasource_service::ImageFilesystemCandidate],
    manifest: &mut staging::StagingManifest,
) -> Result<(PartitionWorkQueue, PartitionBuildFailures), CommandError> {
    let started = Instant::now();
    let mut pending = Vec::new();
    let mut failures = Vec::new();
    for partition in manifest
        .partitions
        .iter()
        .filter(|partition| partition.status != staging::PartitionStatus::Done)
    {
        match build_partition_work(
            &ctx.import_config.source_path,
            &ctx.import_config.kind,
            partition.index,
            &partition.name,
            &partition.fs_kind,
            candidates,
        ) {
            Some(work) => pending.push(work),
            None => failures.push((
                partition.index,
                format!(
                    "Partition {} ({}): could not build filesystem reader",
                    partition.index, partition.name
                ),
            )),
        }
    }
    report_reader_build(ctx, data_source, started, pending.len(), failures.len());
    persist_build_failures(ctx, manifest, &failures)?;
    Ok((pending, failures))
}

fn persist_build_failures(
    ctx: &mut ImportJobContext<'_>,
    manifest: &mut staging::StagingManifest,
    failures: &[(usize, String)],
) -> Result<(), CommandError> {
    if failures.is_empty() {
        return Ok(());
    }
    ctx.counts.add_warnings(failures.len());
    ctx.counts.add_failed(failures.len() as u32);
    for (index, error) in failures {
        tracing::warn!("{error}");
        if let Some(partition) = manifest
            .partitions
            .iter_mut()
            .find(|partition| partition.index == *index)
        {
            partition.status = staging::PartitionStatus::Failed;
            partition.error = Some(error.clone());
        }
    }
    manifest
        .save(ctx.case_root)
        .map_err(CommandError::from_service_error)
}

fn handle_no_pending_partitions(
    ctx: &mut ImportJobContext<'_>,
    manifest: &staging::StagingManifest,
    failures: PartitionBuildFailures,
) -> Result<file_service::EnumerationStats, CommandError> {
    let done_count = manifest
        .partitions
        .iter()
        .filter(|partition| partition.status == staging::PartitionStatus::Done)
        .count();
    if done_count == 0 && !failures.is_empty() {
        persist_outcome_counts(ctx)?;
        return Err(CommandError::internal(
            "No supported partitions could be enumerated",
        ));
    }
    Ok(manifest_stats(manifest))
}

fn enumerate_pending_partitions(
    ctx: &mut ImportJobContext<'_>,
    data_source: &domain::DataSource,
    manifest: &mut staging::StagingManifest,
    pending: PartitionWorkQueue,
) -> Result<(), CommandError> {
    let max_workers = crate::parallel_enum::resolve_worker_count(ctx.options.max_import_workers);
    let active_workers = crate::parallel_enum::effective_worker_count(&pending, max_workers);
    let started = Instant::now();
    let results = run_parallel_enumeration(ctx, data_source, manifest, pending, max_workers)?;
    if ctx.cancel_requested() {
        mark_draining_enumeration(ctx);
    }
    report_enumeration_complete(
        ctx,
        data_source,
        &results,
        active_workers,
        started.elapsed(),
    );
    apply_partition_results(manifest, &results);
    manifest.phase = staging::ImportPhase::Enumerating;
    manifest
        .save(ctx.case_root)
        .map_err(CommandError::from_service_error)?;
    validate_successful_results(ctx, &results)
}
fn run_parallel_enumeration(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
    manifest: &staging::StagingManifest,
    pending: PartitionWorkQueue,
    max_workers: usize,
) -> Result<Vec<crate::parallel_enum::PartitionResult>, CommandError> {
    let total_partitions = manifest.partitions.len() as u32;
    crate::parallel_enum::enumerate_partitions_parallel(
        ctx.case_root,
        &data_source.id,
        pending,
        max_workers,
        Arc::clone(ctx.options.cancel_token),
        &|partition_index, percent, detail| {
            let overall = 25 + (percent * 35 / 100);
            crate::import_pipeline::emit::emit_job_progress(
                ctx.event_sink(),
                &ctx.job_id.0,
                overall.min(60),
                detail,
            );
            crate::import_pipeline::emit::emit_partition_progress(
                ctx.event_sink(),
                &ctx.job_id.0,
                &format!("Partition {partition_index}"),
                partition_index as u32,
                total_partitions,
                percent,
            );
        },
    )
    .map_err(CommandError::from_service_error)
}
fn apply_partition_results(
    manifest: &mut staging::StagingManifest,
    results: &[crate::parallel_enum::PartitionResult],
) {
    for result in results {
        let Some(partition) = manifest
            .partitions
            .iter_mut()
            .find(|partition| partition.index == result.index)
        else {
            continue;
        };
        if let Some(error) = &result.error {
            partition.status = staging::PartitionStatus::Failed;
            partition.error = Some(error.clone());
        } else {
            partition.status = staging::PartitionStatus::Done;
            partition.file_count = result.file_count;
            partition.dir_count = result.dir_count;
            partition.total_size = result.total_size;
            partition.completed_at = Some(chrono::Utc::now().to_rfc3339());
        }
    }
}

fn validate_successful_results(
    ctx: &mut ImportJobContext<'_>,
    results: &[crate::parallel_enum::PartitionResult],
) -> Result<(), CommandError> {
    let failed = results
        .iter()
        .filter(|result| result.error.is_some())
        .count();
    ctx.counts.add_failed(failed as u32);
    if results.len() > failed {
        return Ok(());
    }
    ctx.counts.add_warnings(failed);
    persist_outcome_counts(ctx)?;
    Err(CommandError::internal(
        "No supported partitions could be enumerated",
    ))
}

fn report_reader_build(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
    started: Instant,
    pending: usize,
    failures: usize,
) {
    emit_phase_profile(
        ctx.event_sink(),
        ctx.job_id,
        ctx.case_id,
        Some(&data_source.id),
        31,
        format!(
            "Reader build complete: phase=reader-build elapsedMs={} pending={} failures={} rssMb={}",
            elapsed_ms(started.elapsed()),
            pending,
            failures,
            crate::runtime_resources::current_rss_mb()
        ),
        ctx.cancel_requested(),
    );
}

fn report_enumeration_complete(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
    results: &[crate::parallel_enum::PartitionResult],
    workers: usize,
    elapsed: std::time::Duration,
) {
    let files: u64 = results.iter().map(|result| result.file_count).sum();
    let dirs: u64 = results.iter().map(|result| result.dir_count).sum();
    let bytes: u64 = results.iter().map(|result| result.total_size).sum();
    emit_phase_profile(
        ctx.event_sink(),
        ctx.job_id,
        ctx.case_id,
        Some(&data_source.id),
        60,
        format!(
            "Enumeration complete: phase=enumeration elapsedMs={} rows={} rowsPerSec={} dataMb={} mbPerSec={} workers={} rssMb={}",
            elapsed_ms(elapsed),
            files + dirs,
            rows_per_sec(files + dirs, elapsed),
            bytes_to_mb(bytes),
            mb_per_sec(bytes, elapsed),
            workers,
            crate::runtime_resources::current_rss_mb()
        ),
        ctx.cancel_requested(),
    );
}

fn populate_file_graph(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
    stats: &mut file_service::EnumerationStats,
) {
    let Some(source_conn) = ctx.source_conn else {
        return;
    };
    if let Err(error) =
        file_service::populate_file_graph_for_data_source(source_conn, &data_source.id)
    {
        let warning = format!("Graph population warning: {error}");
        tracing::warn!(%warning, "Failed to populate file graph after enumeration");
        stats.warnings.push(warning);
    }
}

fn report_catalog_ready(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
    stats: &file_service::EnumerationStats,
) {
    emit_phase_profile(
        ctx.event_sink(),
        ctx.job_id,
        ctx.case_id,
        Some(&data_source.id),
        70,
        format!(
            "File catalog ready: phase=enum-merge rows={} files={} dirs={} warnings={} rssMb={}",
            stats.file_count + stats.dir_count,
            stats.file_count,
            stats.dir_count,
            stats.warnings.len(),
            crate::runtime_resources::current_rss_mb()
        ),
        ctx.cancel_requested(),
    );
}

fn reject_cancelled_before_analysis(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
) -> Result<(), CommandError> {
    if !ctx.cancel_requested() {
        return Ok(());
    }
    mark_import_cancelling(
        &ctx.job_repo,
        ctx.job_id,
        "Cancellation acknowledged before post-import analysis",
    );
    emit_import_cancellation_state(
        ctx.event_sink(),
        ctx.job_id,
        transport::dto::CancellationStateDto::Acknowledged,
        false,
        "Cancellation acknowledged before post-import analysis",
    );
    emit_import_profile_progress(
        ctx.event_sink(),
        ctx.job_id,
        ctx.case_id,
        Some(&data_source.id),
        70,
        "Cancellation acknowledged: phase=enumeration",
        true,
    );
    Err(CommandError::internal("Import cancelled by user"))
}

fn reject_cancelled_after_enumeration(ctx: &mut ImportJobContext<'_>) -> Result<(), CommandError> {
    if !ctx.cancel_requested() {
        return Ok(());
    }
    ctx.job_repo
        .update_outcome_counts(
            ctx.job_id,
            ctx.counts.warning_count,
            ctx.counts.skipped_count.saturating_add(1),
            ctx.counts.failed_count,
            true,
        )
        .map_err(CommandError::from_service_error)?;
    emit_import_cancellation_state(
        ctx.event_sink(),
        ctx.job_id,
        transport::dto::CancellationStateDto::Acknowledged,
        false,
        "Import cancellation acknowledged after enumeration",
    );
    Err(CommandError::internal("Import cancelled by user"))
}

fn mark_draining_enumeration(ctx: &ImportJobContext<'_>) {
    mark_import_cancelling(
        &ctx.job_repo,
        ctx.job_id,
        "Cancellation acknowledged; draining enumeration workers",
    );
    emit_import_cancellation_state(
        ctx.event_sink(),
        ctx.job_id,
        transport::dto::CancellationStateDto::Draining,
        false,
        "Cancellation acknowledged; draining enumeration workers",
    );
}

fn persist_outcome_counts(ctx: &ImportJobContext<'_>) -> Result<(), CommandError> {
    ctx.job_repo
        .update_outcome_counts(
            ctx.job_id,
            ctx.counts.warning_count,
            ctx.counts.skipped_count,
            ctx.counts.failed_count,
            ctx.counts.is_partial(),
        )
        .map_err(CommandError::from_service_error)
}

fn manifest_stats(manifest: &staging::StagingManifest) -> file_service::EnumerationStats {
    file_service::EnumerationStats {
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
        warnings: Vec::new(),
        diagnostics: Vec::new(),
    }
}
