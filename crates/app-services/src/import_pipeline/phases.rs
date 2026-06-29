//! Per-phase import pipeline logic.
//!
//! The functions in this module are the extracted bodies of the original
//! monolithic `execute_import_job_with_counts` routine. Each function is
//! responsible for exactly one conceptual phase:
//!
//! 1. `run_attach_phase`     - attach the data source to the case.
//! 2. `run_enumeration_phase` - enumerate files (logical dir or image partitions).
//! 3. `run_post_import_phase` - run timeline / artifact / search indexing.
//! 4. `run_finalize_phase`    - emit final events and build the summary message.
//!
//! Shared state lives in [`ImportJobContext`](crate::import_pipeline::types::ImportJobContext).

use std::sync::{Arc, Mutex};
use std::time::Instant;

use evidence_core::{EvidenceReader, LogicalFsReader, RawImageReader};
use image_e01::E01Reader;
use transport::CommandError;

use crate::{datasource_service, file_service, import_analysis, staging, step_recorder};

use crate::import_pipeline::{
    execute::{
        bytes_to_mb, elapsed_ms, emit_import_cancellation_state, emit_import_profile_progress,
        emit_phase_profile, is_import_cancelled_message, mark_import_cancelling, mb_per_sec,
        post_import_counts_from_message, rows_per_sec,
    },
    options::JobOutcomeCounts,
    partition::{
        build_partition_work, format_partition_record_root_name, format_partition_root_name,
    },
    types::{ImportJobContext, PhaseTelemetry},
};

/// Shorthand for the pending partition work queue returned by the reader-build
/// step. Aliased only to keep the function signature readable.
type PartitionWorkQueue = Vec<crate::parallel_enum::PartitionWork>;

/// Shorthand for partition reader-build failures (index, message).
type PartitionBuildFailures = Vec<(usize, String)>;

/// Attach the data source to the case and report initial progress.
pub(crate) fn run_attach_phase(
    ctx: &mut ImportJobContext<'_>,
) -> Result<domain::DataSource, CommandError> {
    let source_name = ctx.import_config.source_name.clone();
    let path = ctx.import_config.source_path.clone();
    let kind = ctx.import_config.kind.clone();

    ctx.report_job_progress(10, &format!("Attaching data source {source_name}"))?;

    let telemetry = PhaseTelemetry::new();
    let ds = datasource_service::attach_data_source(
        ctx.conn,
        ctx.case_id,
        &source_name,
        &path,
        kind.clone(),
    )
    .map_err(CommandError::from_service_error)?;

    emit_phase_profile(
        ctx.app(),
        ctx.job_id,
        ctx.case_id,
        Some(&ds.id),
        12,
        format!(
            "Attach complete: phase=attach elapsedMs={} rssMb={}",
            telemetry.elapsed_ms(),
            import_analysis::current_rss_mb()
        ),
        ctx.cancel_requested(),
    );

    Ok(ds)
}

/// Enumerate the filesystem and merge staging output into the main database.
pub(crate) fn run_enumeration_phase(
    ctx: &mut ImportJobContext<'_>,
    ds: &domain::DataSource,
) -> Result<file_service::EnumerationStats, CommandError> {
    ctx.report_job_progress(25, "Enumerating filesystem...")?;

    let stats = match ctx.import_config.kind {
        domain::DataSourceKind::LogicalDirectory => {
            enumerate_logical_directory(ctx, ds).map_err(CommandError::from_service_error)?
        }
        domain::DataSourceKind::E01 | domain::DataSourceKind::Raw => {
            enumerate_image_data_source_with_staging(ctx, ds)?
        }
    };

    ctx.counts.add_warnings(stats.warnings.len());
    emit_phase_profile(
        ctx.app(),
        ctx.job_id,
        ctx.case_id,
        Some(&ds.id),
        70,
        format!(
            "File catalog ready: phase=enum-merge rows={} files={} dirs={} warnings={} rssMb={}",
            stats.file_count + stats.dir_count,
            stats.file_count,
            stats.dir_count,
            stats.warnings.len(),
            import_analysis::current_rss_mb()
        ),
        ctx.cancel_requested(),
    );

    if ctx.cancel_requested() {
        mark_import_cancelling(
            &ctx.job_repo,
            ctx.job_id,
            "Cancellation acknowledged before post-import analysis",
        );
        emit_import_cancellation_state(
            ctx.app(),
            ctx.job_id,
            transport::dto::CancellationStateDto::Acknowledged,
            false,
            "Cancellation acknowledged before post-import analysis",
        );
        emit_import_profile_progress(
            ctx.app(),
            ctx.job_id,
            ctx.case_id,
            Some(&ds.id),
            70,
            "Cancellation acknowledged: phase=enumeration",
            true,
        );
        return Err(CommandError::internal("Import cancelled by user"));
    }

    Ok(stats)
}

fn enumerate_logical_directory(
    ctx: &mut ImportJobContext<'_>,
    ds: &domain::DataSource,
) -> Result<file_service::EnumerationStats, persistence_sqlite::DbError> {
    let path = &ctx.import_config.source_path;
    let fs = LogicalFsReader::open(path, &ds.name)?;
    file_service::enumerate_filesystem_with_root_name_and_cancel(
        ctx.conn,
        &ds.id,
        &fs,
        None,
        None::<&dyn Fn(u32)>,
        Some(ctx.options.cancel_token),
    )
}

fn enumerate_image_data_source_with_staging(
    ctx: &mut ImportJobContext<'_>,
    ds: &domain::DataSource,
) -> Result<file_service::EnumerationStats, CommandError> {
    let path = ctx.import_config.source_path.clone();
    let kind = ctx.import_config.kind.clone();
    let case_root = ctx.case_root;

    // Load or create manifest for staging-based import.
    let mut manifest = staging::StagingManifest::load(case_root, &ds.id.0).unwrap_or_else(|| {
        staging::StagingManifest::create(
            &ds.id.0,
            ctx.source_path,
            ctx.import_config.staging_kind().unwrap_or("Raw"),
        )
    });

    let mut probe_candidates: Vec<datasource_service::ImageFilesystemCandidate> = Vec::new();

    if manifest.partitions.is_empty() {
        probe_and_seed_manifest(
            ctx,
            ds,
            &path,
            &kind,
            case_root,
            &mut manifest,
            &mut probe_candidates,
        )?;
    }

    if manifest.partitions.is_empty() {
        return Ok(file_service::EnumerationStats {
            file_count: 0,
            dir_count: 0,
            total_size: 0,
            warnings: vec![],
        });
    }

    // Update partition statuses from staging DBs (for resume).
    refresh_partition_statuses_from_staging(case_root, &ds.id.0, &mut manifest)?;

    if let Some(app) = ctx.app() {
        crate::import_pipeline::emit::emit_job_progress(
            app,
            &ctx.job_id.0,
            30,
            "Building filesystem readers...",
        );
    }

    // For resume: probe once to get candidates if we don't have them.
    if probe_candidates.is_empty() {
        probe_resume(ctx, ds, &path, &kind, &mut probe_candidates)?;
    }

    // Build work items for pending partitions — reuse probe results, no re-probe.
    let (pending, build_failures) =
        build_pending_partition_work(ctx, ds, &path, &kind, &probe_candidates, &mut manifest)?;

    if pending.is_empty() {
        return handle_no_pending_partitions(ctx, &manifest, build_failures);
    }

    enumerate_pending_partitions(ctx, ds, case_root, &mut manifest, pending)?;

    if ctx.cancel_requested() {
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
            ctx.app(),
            ctx.job_id,
            transport::dto::CancellationStateDto::Acknowledged,
            false,
            "Import cancellation acknowledged after enumeration",
        );
        return Err(CommandError::internal("Import cancelled by user"));
    }

    let final_stats = merge_enumeration_results(ctx, ds, case_root, &manifest)?;
    Ok(final_stats)
}

fn probe_and_seed_manifest(
    ctx: &mut ImportJobContext<'_>,
    ds: &domain::DataSource,
    path: &std::path::Path,
    kind: &domain::DataSourceKind,
    case_root: &std::path::Path,
    manifest: &mut staging::StagingManifest,
    probe_candidates: &mut Vec<datasource_service::ImageFilesystemCandidate>,
) -> Result<(), CommandError> {
    let probe_started = Instant::now();
    let mut probe_reader: Box<dyn EvidenceReader> = if *kind == domain::DataSourceKind::E01 {
        Box::new(E01Reader::open(path).map_err(CommandError::from_service_error)?)
    } else {
        Box::new(RawImageReader::open(path).map_err(CommandError::from_service_error)?)
    };
    let probe = datasource_service::detect_image_filesystem(&mut probe_reader)
        .map_err(CommandError::from_service_error)?;

    emit_phase_profile(
        ctx.app(),
        ctx.job_id,
        ctx.case_id,
        Some(&ds.id),
        28,
        format!(
            "Probe complete: phase=probe elapsedMs={} partitions={} candidates={} rssMb={}",
            elapsed_ms(probe_started.elapsed()),
            probe.partitions.len(),
            probe.candidates.len(),
            import_analysis::current_rss_mb()
        ),
        ctx.cancel_requested(),
    );

    // Store partition records in main DB.
    file_service::store_data_source_partitions(ctx.conn, &ds.id, &probe.partitions)
        .map_err(CommandError::from_service_error)?;

    let candidate_index_map =
        datasource_service::assign_effective_partition_indices(&probe.candidates);

    let candidate_root_names = probe
        .candidates
        .iter()
        .enumerate()
        .map(|(i, candidate)| {
            let index =
                datasource_service::effective_partition_index(candidate, i, &candidate_index_map);
            (index, format_partition_root_name(candidate))
        })
        .collect::<std::collections::HashMap<usize, String>>();

    for partition in &probe.partitions {
        let status = match partition.status {
            datasource_service::PartitionStatus::Supported => "queued",
            datasource_service::PartitionStatus::EncryptedBitLocker => "locked",
            datasource_service::PartitionStatus::Unsupported => "unsupported",
        };
        let root_name = candidate_root_names
            .get(&partition.index)
            .cloned()
            .unwrap_or_else(|| format_partition_record_root_name(partition));
        file_service::insert_partition_placeholder_root(
            ctx.conn,
            &ds.id,
            partition.index,
            &root_name,
            status,
        )
        .map_err(CommandError::from_service_error)?;
    }

    // Build manifest entries for supported partitions.
    for (i, candidate) in probe.candidates.iter().enumerate() {
        let index =
            datasource_service::effective_partition_index(candidate, i, &candidate_index_map);
        let name = format_partition_root_name(candidate);
        manifest.partitions.push(staging::PartitionEntry {
            index,
            name,
            fs_kind: format!("{:?}", candidate.kind),
            staging_db: format!("enum_partition_{index}.db"),
            status: staging::PartitionStatus::Pending,
            file_count: 0,
            dir_count: 0,
            total_size: 0,
            last_path: None,
            completed_at: None,
            error: None,
        });
    }
    *probe_candidates = probe.candidates;
    manifest
        .save(case_root)
        .map_err(CommandError::from_service_error)?;

    Ok(())
}

fn refresh_partition_statuses_from_staging(
    case_root: &std::path::Path,
    ds_id: &str,
    manifest: &mut staging::StagingManifest,
) -> Result<(), CommandError> {
    for partition in &mut manifest.partitions {
        let staging_db_path = staging::staging_db_path(case_root, ds_id, partition.index);
        if staging_db_path.exists() {
            if let Ok(staging_conn) =
                staging::open_partition_staging(case_root, ds_id, partition.index)
            {
                if let Ok(Some(status)) = staging::get_staging_meta(&staging_conn, "status") {
                    match status.as_str() {
                        "done" => {
                            partition.status = staging::PartitionStatus::Done;
                            partition.file_count =
                                staging::staging_db_row_count(&staging_conn).unwrap_or(0);
                        }
                        "failed" => {
                            partition.status = staging::PartitionStatus::Failed;
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    manifest
        .save(case_root)
        .map_err(CommandError::from_service_error)
}

fn probe_resume(
    ctx: &mut ImportJobContext<'_>,
    ds: &domain::DataSource,
    path: &std::path::Path,
    kind: &domain::DataSourceKind,
    probe_candidates: &mut Vec<datasource_service::ImageFilesystemCandidate>,
) -> Result<(), CommandError> {
    let probe_started = Instant::now();
    let mut probe_reader: Box<dyn EvidenceReader> = if *kind == domain::DataSourceKind::E01 {
        Box::new(E01Reader::open(path).map_err(CommandError::from_service_error)?)
    } else {
        Box::new(RawImageReader::open(path).map_err(CommandError::from_service_error)?)
    };
    let probe = datasource_service::detect_image_filesystem(&mut probe_reader)
        .map_err(CommandError::from_service_error)?;
    emit_phase_profile(
        ctx.app(),
        ctx.job_id,
        ctx.case_id,
        Some(&ds.id),
        30,
        format!(
            "Probe complete: phase=probe-resume elapsedMs={} candidates={} rssMb={}",
            elapsed_ms(probe_started.elapsed()),
            probe.candidates.len(),
            import_analysis::current_rss_mb()
        ),
        ctx.cancel_requested(),
    );
    *probe_candidates = probe.candidates;
    Ok(())
}

fn build_pending_partition_work(
    ctx: &mut ImportJobContext<'_>,
    ds: &domain::DataSource,
    path: &std::path::Path,
    kind: &domain::DataSourceKind,
    probe_candidates: &[datasource_service::ImageFilesystemCandidate],
    manifest: &mut staging::StagingManifest,
) -> Result<(PartitionWorkQueue, PartitionBuildFailures), CommandError> {
    let build_started = Instant::now();
    let mut pending: Vec<crate::parallel_enum::PartitionWork> = Vec::new();
    let mut build_failures = Vec::new();

    for p in manifest.partitions.iter() {
        if p.status == staging::PartitionStatus::Done {
            continue;
        }
        let work = build_partition_work(path, kind, p.index, &p.name, &p.fs_kind, probe_candidates);
        match work {
            Some(w) => pending.push(w),
            None => {
                let error = format!(
                    "Partition {} ({}): could not build filesystem reader",
                    p.index, p.name
                );
                tracing::warn!("{}", error);
                build_failures.push((p.index, error));
            }
        }
    }

    emit_phase_profile(
        ctx.app(),
        ctx.job_id,
        ctx.case_id,
        Some(&ds.id),
        31,
        format!(
            "Reader build complete: phase=reader-build elapsedMs={} pending={} failures={} rssMb={}",
            elapsed_ms(build_started.elapsed()),
            pending.len(),
            build_failures.len(),
            import_analysis::current_rss_mb()
        ),
        ctx.cancel_requested(),
    );

    if !build_failures.is_empty() {
        ctx.counts.add_warnings(build_failures.len());
        ctx.counts.add_failed(build_failures.len() as u32);
        for (index, error) in &build_failures {
            if let Some(partition) = manifest.partitions.iter_mut().find(|p| p.index == *index) {
                partition.status = staging::PartitionStatus::Failed;
                partition.error = Some(error.clone());
            }
        }
        manifest
            .save(ctx.case_root)
            .map_err(CommandError::from_service_error)?;
    }

    Ok((pending, build_failures))
}

fn handle_no_pending_partitions(
    ctx: &mut ImportJobContext<'_>,
    manifest: &staging::StagingManifest,
    build_failures: Vec<(usize, String)>,
) -> Result<file_service::EnumerationStats, CommandError> {
    let done_count = manifest
        .partitions
        .iter()
        .filter(|p| p.status == staging::PartitionStatus::Done)
        .count();
    if done_count == 0 && !build_failures.is_empty() {
        ctx.job_repo
            .update_outcome_counts(
                ctx.job_id,
                ctx.counts.warning_count,
                ctx.counts.skipped_count,
                ctx.counts.failed_count,
                ctx.counts.is_partial(),
            )
            .map_err(CommandError::from_service_error)?;
        return Err(CommandError::internal(
            "No supported partitions could be enumerated",
        ));
    }
    Ok(file_service::EnumerationStats {
        file_count: manifest.partitions.iter().map(|p| p.file_count).sum(),
        dir_count: manifest.partitions.iter().map(|p| p.dir_count).sum(),
        total_size: manifest.partitions.iter().map(|p| p.total_size).sum(),
        warnings: vec![],
    })
}

fn enumerate_pending_partitions(
    ctx: &mut ImportJobContext<'_>,
    ds: &domain::DataSource,
    case_root: &std::path::Path,
    manifest: &mut staging::StagingManifest,
    pending: Vec<crate::parallel_enum::PartitionWork>,
) -> Result<(), CommandError> {
    let max_workers = crate::parallel_enum::resolve_worker_count(ctx.options.max_import_workers);
    let ds_id_clone = ds.id.clone();
    let app_ref = ctx.options.app;
    let job_ref = ctx.job_id;
    let case_root_clone = case_root.to_path_buf();
    let total_partitions = manifest.partitions.len() as u32;

    let enum_started = Instant::now();
    let results = crate::parallel_enum::enumerate_partitions_parallel(
        &case_root_clone,
        &ds_id_clone,
        pending,
        max_workers,
        Arc::clone(ctx.options.cancel_token),
        &|partition_idx, pct, detail| {
            if let Some(a) = app_ref {
                let overall = 25 + (pct * 35 / 100);
                crate::import_pipeline::emit::emit_job_progress(
                    a,
                    &job_ref.0,
                    overall.min(60),
                    detail,
                );
                crate::import_pipeline::emit::emit_partition_progress(
                    a,
                    &job_ref.0,
                    &format!("Partition {}", partition_idx),
                    partition_idx as u32,
                    total_partitions,
                    pct,
                );
            }
        },
    )
    .map_err(CommandError::from_service_error)?;
    let enum_elapsed = enum_started.elapsed();
    let enum_files: u64 = results.iter().map(|result| result.file_count).sum();
    let enum_dirs: u64 = results.iter().map(|result| result.dir_count).sum();
    let enum_size: u64 = results.iter().map(|result| result.total_size).sum();

    if ctx.cancel_requested() {
        mark_import_cancelling(
            &ctx.job_repo,
            ctx.job_id,
            "Cancellation acknowledged; draining enumeration workers",
        );
        emit_import_cancellation_state(
            ctx.app(),
            ctx.job_id,
            transport::dto::CancellationStateDto::Draining,
            false,
            "Cancellation acknowledged; draining enumeration workers",
        );
    }

    emit_phase_profile(
        ctx.app(),
        ctx.job_id,
        ctx.case_id,
        Some(&ds.id),
        60,
        format!(
            "Enumeration complete: phase=enumeration elapsedMs={} rows={} rowsPerSec={} dataMb={} mbPerSec={} workers={} rssMb={}",
            elapsed_ms(enum_elapsed),
            enum_files + enum_dirs,
            rows_per_sec(enum_files + enum_dirs, enum_elapsed),
            bytes_to_mb(enum_size),
            mb_per_sec(enum_size, enum_elapsed),
            max_workers,
            import_analysis::current_rss_mb()
        ),
        ctx.cancel_requested(),
    );

    // Update manifest with results.
    for result in &results {
        if let Some(p) = manifest
            .partitions
            .iter_mut()
            .find(|p| p.index == result.index)
        {
            if result.error.is_some() {
                p.status = staging::PartitionStatus::Failed;
                p.error = result.error.clone();
            } else {
                p.status = staging::PartitionStatus::Done;
                p.file_count = result.file_count;
                p.dir_count = result.dir_count;
                p.total_size = result.total_size;
                p.completed_at = Some(chrono::Utc::now().to_rfc3339());
            }
        }
    }
    let failed_results = results
        .iter()
        .filter(|result| result.error.is_some())
        .count();
    if failed_results > 0 {
        ctx.counts.add_failed(failed_results as u32);
    }
    manifest.phase = staging::ImportPhase::Enumerating;
    manifest
        .save(case_root)
        .map_err(CommandError::from_service_error)?;

    let success_results = results
        .iter()
        .filter(|result| result.error.is_none())
        .count();
    if success_results == 0 {
        ctx.counts.add_warnings(failed_results);
        ctx.job_repo
            .update_outcome_counts(
                ctx.job_id,
                ctx.counts.warning_count,
                ctx.counts.skipped_count,
                ctx.counts.failed_count,
                ctx.counts.is_partial(),
            )
            .map_err(CommandError::from_service_error)?;
        return Err(CommandError::internal(
            "No supported partitions could be enumerated",
        ));
    }

    Ok(())
}

fn merge_enumeration_results(
    ctx: &mut ImportJobContext<'_>,
    ds: &domain::DataSource,
    case_root: &std::path::Path,
    manifest: &staging::StagingManifest,
) -> Result<file_service::EnumerationStats, CommandError> {
    if let Some(a) = ctx.app() {
        crate::import_pipeline::emit::emit_job_progress(
            a,
            &ctx.job_id.0,
            62,
            "Merging partitions...",
        );
    }

    // Note: manifest is mutated only to record the phase; the caller already
    // owns the mutable manifest, but this helper just needs read access for the
    // merge itself.
    let mut manifest_for_save = manifest.clone();
    manifest_for_save.phase = staging::ImportPhase::Merging;
    manifest_for_save
        .save(case_root)
        .map_err(CommandError::from_service_error)?;

    let enum_merge_started = Instant::now();
    let merged = staging::merge_all_staging_to_main(
        ctx.conn,
        case_root,
        &ds.id.0,
        manifest,
        Some(&|completed, total| {
            if let Some(a) = ctx.app() {
                let pct = 62 + (completed as u32 * 8 / total as u32);
                crate::import_pipeline::emit::emit_job_progress(
                    a,
                    &ctx.job_id.0,
                    pct.min(70),
                    &format!("Merged {}/{} partitions", completed, total),
                );
            }
        }),
    )
    .map_err(CommandError::from_service_error)?;
    let enum_merge_elapsed = enum_merge_started.elapsed();

    emit_phase_profile(
        ctx.app(),
        ctx.job_id,
        ctx.case_id,
        Some(&ds.id),
        70,
        format!(
            "Partition merge complete: phase=enum-merge elapsedMs={} rows={} rowsPerSec={} rssMb={}",
            elapsed_ms(enum_merge_elapsed),
            merged,
            rows_per_sec(merged, enum_merge_elapsed),
            import_analysis::current_rss_mb()
        ),
        ctx.cancel_requested(),
    );

    // We need the original results to build warnings. Re-open each staging DB
    // to collect the per-partition warnings/errors.
    let warnings: Vec<String> = manifest
        .partitions
        .iter()
        .filter_map(|p| {
            if let Some(error) = &p.error {
                return Some(format!("Partition {}: {}", p.index, error));
            }
            let staging_db_path = staging::staging_db_path(case_root, &ds.id.0, p.index);
            if !staging_db_path.exists() {
                return None;
            }
            let staging_conn =
                staging::open_partition_staging(case_root, &ds.id.0, p.index).ok()?;
            let mut partition_warnings: Vec<String> = Vec::new();
            if let Ok(Some(warning_blob)) = staging::get_staging_meta(&staging_conn, "warnings") {
                for warning in warning_blob.split('\n') {
                    if !warning.is_empty() {
                        partition_warnings.push(format!("Partition {}: {}", p.index, warning));
                    }
                }
            }
            Some(partition_warnings)
                .filter(|list| !list.is_empty())
                .map(|list| list.join("\n"))
        })
        .collect::<Vec<String>>()
        .join("\n")
        .lines()
        .map(|line| line.to_string())
        .collect();

    Ok(file_service::EnumerationStats {
        file_count: manifest.partitions.iter().map(|p| p.file_count).sum(),
        dir_count: manifest.partitions.iter().map(|p| p.dir_count).sum(),
        total_size: manifest.partitions.iter().map(|p| p.total_size).sum(),
        warnings,
    })
}

/// Run post-import analysis (timeline, artifacts, search indexing).
pub(crate) fn run_post_import_phase(
    ctx: &mut ImportJobContext<'_>,
    ds: &domain::DataSource,
) -> Result<String, CommandError> {
    ctx.report_job_progress(70, "Running post-import pipeline...")?;

    let post_import_db_path = ctx.case_root.join("app.db");
    let index_dir = ctx.case_root.join("indexes").join("tantivy");
    let image_backed_source = ctx.import_config.is_image_backed();
    let analysis_mode = if image_backed_source {
        ctx.options.analysis_mode
    } else {
        match ctx.options.analysis_mode {
            import_analysis::ImportAnalysisMode::MetadataOnly => {
                import_analysis::ImportAnalysisMode::BudgetedContent
            }
            mode => mode,
        }
    };

    let post_import_started = Instant::now();
    let progress_adapter = |pct: u32, detail: &str| {
        emit_import_profile_progress(
            ctx.app(),
            ctx.job_id,
            ctx.case_id,
            Some(&ds.id),
            pct,
            detail,
            ctx.cancel_requested(),
        );
    };

    let (pipeline_msg, pipeline_counts) = import_analysis::run_post_import_pipeline_with_counts(
        import_analysis::PostImportPipelineOptions {
            case_root: ctx.case_root.to_path_buf(),
            db_path: post_import_db_path,
            case_id: ctx.case_id.0.clone(),
            data_source_id: ds.id.clone(),
            index_dir,
            max_analysis_workers: ctx.options.max_analysis_workers,
            cancel_token: Arc::clone(ctx.options.cancel_token),
            enable_timeline_projection: !image_backed_source,
            enable_content_extraction: analysis_mode.allows_content(),
            enable_text_indexing: analysis_mode.allows_content(),
            analysis_mode,
            tier_state: Arc::new(Mutex::new(import_analysis::tier::TierStateMachine::new())),
        },
        Some(&progress_adapter),
    )
    .map_err(|error| {
        let service_counts = JobOutcomeCounts::from(error.counts);
        ctx.counts.warning_count = ctx
            .counts
            .warning_count
            .saturating_add(service_counts.warning_count);
        ctx.counts.skipped_count = ctx
            .counts
            .skipped_count
            .saturating_add(service_counts.skipped_count);
        ctx.counts.failed_count = ctx
            .counts
            .failed_count
            .saturating_add(service_counts.failed_count);
        let cancellation_error =
            ctx.cancel_requested() || is_import_cancelled_message(&error.message);
        if cancellation_error {
            mark_import_cancelling(
                &ctx.job_repo,
                ctx.job_id,
                "Cancellation acknowledged during post-import analysis drain",
            );
            emit_import_cancellation_state(
                ctx.app(),
                ctx.job_id,
                transport::dto::CancellationStateDto::Draining,
                false,
                "Cancellation acknowledged during post-import analysis drain",
            );
            CommandError::internal("Import cancelled by user")
        } else {
            CommandError::from_service_error(error.message)
        }
    })?;

    let pipeline_counts = JobOutcomeCounts::from(pipeline_counts);
    let post_import_results = post_import_counts_from_message(&pipeline_msg);

    emit_phase_profile(
        ctx.app(),
        ctx.job_id,
        ctx.case_id,
        Some(&ds.id),
        94,
        format!(
            "Post-import complete: phase=post-import elapsedMs={} timeline={} artifacts={} indexed={} rssMb={}",
            elapsed_ms(post_import_started.elapsed()),
            post_import_results.timeline_events,
            post_import_results.artifact_count,
            post_import_results.indexed_count,
            import_analysis::current_rss_mb()
        ),
        ctx.cancel_requested(),
    );

    ctx.counts.warning_count = ctx
        .counts
        .warning_count
        .saturating_add(pipeline_counts.warning_count);
    ctx.counts.skipped_count = ctx
        .counts
        .skipped_count
        .saturating_add(pipeline_counts.skipped_count);
    ctx.counts.failed_count = ctx
        .counts
        .failed_count
        .saturating_add(pipeline_counts.failed_count);

    ctx.job_repo
        .update_outcome_counts(
            ctx.job_id,
            ctx.counts.warning_count,
            ctx.counts.skipped_count,
            ctx.counts.failed_count,
            ctx.counts.is_partial(),
        )
        .map_err(CommandError::from_service_error)?;

    Ok(pipeline_msg)
}

/// Emit final events, build the summary message, and record provenance.
pub(crate) fn run_finalize_phase(
    ctx: &mut ImportJobContext<'_>,
    ds: &domain::DataSource,
    stats: &file_service::EnumerationStats,
    pipeline_msg: &str,
    import_started: Instant,
) -> Result<String, CommandError> {
    if let Some(app) = ctx.app() {
        crate::import_pipeline::emit::emit_timeline_updated(
            app,
            stats.file_count + stats.dir_count,
        );
        crate::import_pipeline::emit::emit_search_index_progress(
            app,
            100,
            "Post-import indexing completed",
        );
    }

    ctx.report_job_progress(95, "Finalizing...")?;

    if let Some(app) = ctx.app() {
        match file_service::get_data_sources_real(ctx.conn, ctx.case_id)
            .map_err(CommandError::from_service_error)?
            .into_iter()
            .find(|source| source.id == ds.id.0)
        {
            Some(summary) => {
                crate::import_pipeline::emit::emit_data_source_imported(
                    app,
                    &ctx.case_id.0,
                    &summary,
                    &ctx.job_id.0,
                );
            }
            None => tracing::warn!(
                "Imported data source {} was not found in summary list for event emission",
                ds.id.0
            ),
        }
    }

    emit_phase_profile(
        ctx.app(),
        ctx.job_id,
        ctx.case_id,
        Some(&ds.id),
        99,
        format!(
            "Import profile complete: phase=total elapsedMs={} rssMb={}",
            elapsed_ms(import_started.elapsed()),
            import_analysis::current_rss_mb()
        ),
        ctx.cancel_requested(),
    );

    let source_name = &ctx.import_config.source_name;
    let mut msg = format!(
        "Imported {}: {} files, {} dirs",
        source_name, stats.file_count, stats.dir_count
    );
    if !pipeline_msg.is_empty() {
        msg.push_str(". ");
        msg.push_str(pipeline_msg);
    }

    let import_duration_ms = import_started.elapsed().as_millis() as u32;
    let params_json = serde_json::json!({
        "sourcePath": ctx.source_path,
        "sourceName": source_name,
        "kind": format!("{:?}", ctx.import_config.kind),
        "filesEnumerated": stats.file_count,
        "dirsEnumerated": stats.dir_count,
    })
    .to_string();
    let _ = step_recorder::record_step(
        ctx.conn,
        &ctx.case_id.0,
        "import",
        &params_json,
        import_duration_ms,
        true,
        None,
    );

    Ok(msg)
}
