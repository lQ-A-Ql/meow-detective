use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use evidence_core::{EvidenceReader, LogicalFsReader, RawImageReader};
use image_e01::E01Reader;
use persistence_sqlite::repositories::job_repo::JobRepo;
use tauri::AppHandle;
use transport::{
    dto::{
        CancellationStateDto, ImportPhaseDto, ImportPhaseMetricsDto, ImportPhaseProgressDto,
        ImportPhaseStateDto, IndexCacheStatusDto, PartialResultDto, PartialResultKindDto,
        ResultFreshnessDto,
    },
    CommandError,
};

use crate::{
    datasource_service, file_service, import_analysis,
    import_pipeline::{
        emit,
        options::{ImportJobOptions, JobOutcomeCounts},
        partition::{
            build_partition_work, format_partition_record_root_name, format_partition_root_name,
        },
    },
    import_precheck, staging, step_recorder,
};

/// Convert a precheck config error into a command error.
fn import_config_error_to_command_error(
    error: import_precheck::ImportSourceConfigError,
) -> CommandError {
    if error.is_invalid_input() {
        CommandError::invalid_input(error.to_string())
    } else {
        CommandError::from_service_error(error)
    }
}

/// Execute the import job (main logic).
pub fn execute_import_job(
    conn: &rusqlite::Connection,
    case_id: &domain::CaseId,
    case_root: &std::path::Path,
    source_path: &str,
    job_id: &domain::JobId,
    options: ImportJobOptions<'_>,
) -> Result<String, CommandError> {
    let (message, _counts) =
        execute_import_job_with_counts(conn, case_id, case_root, source_path, job_id, options)?;
    Ok(message)
}

/// Execute the import job and return both the summary message and outcome counts.
pub fn execute_import_job_with_counts(
    conn: &rusqlite::Connection,
    case_id: &domain::CaseId,
    case_root: &std::path::Path,
    source_path: &str,
    job_id: &domain::JobId,
    options: ImportJobOptions<'_>,
) -> Result<(String, JobOutcomeCounts), CommandError> {
    let import_config = import_precheck::prepare_import_source_config_from_path(source_path)
        .map_err(import_config_error_to_command_error)?;
    let path = import_config.source_path.clone();
    let kind = import_config.kind.clone();
    let source_name = import_config.source_name.clone();
    let index_dir = case_root.join("indexes").join("tantivy");
    let job_repo = JobRepo::new(conn);
    let mut counts = JobOutcomeCounts::default();
    let import_started = Instant::now();

    job_repo
        .update_progress(job_id, 10, &format!("Attaching data source {source_name}"))
        .map_err(CommandError::from_service_error)?;
    if let Some(app) = options.app {
        emit::emit_job_progress(app, &job_id.0, 10, &format!("Attaching {source_name}"));
    }
    let attach_started = Instant::now();
    let ds =
        datasource_service::attach_data_source(conn, case_id, &source_name, &path, kind.clone())
            .map_err(CommandError::from_service_error)?;
    emit_phase_profile(
        options.app,
        job_id,
        case_id,
        Some(&ds.id),
        12,
        format!(
            "Attach complete: phase=attach elapsedMs={} rssMb={}",
            elapsed_ms(attach_started.elapsed()),
            import_analysis::current_rss_mb()
        ),
        options.cancel_token.load(Ordering::Relaxed),
    );

    // Check for cancellation
    if options.cancel_token.load(Ordering::Relaxed) {
        mark_import_cancelling(&job_repo, job_id, "Cancellation acknowledged after attach");
        emit_import_cancellation_state(
            options.app,
            job_id,
            CancellationStateDto::Acknowledged,
            false,
            "Cancellation acknowledged after attach",
        );
        emit_import_profile_progress(
            options.app,
            job_id,
            case_id,
            Some(&ds.id),
            12,
            "Cancellation acknowledged: phase=attach",
            true,
        );
        return Err(CommandError::internal("Import cancelled by user"));
    }

    job_repo
        .update_progress(job_id, 25, "Enumerating filesystem...")
        .map_err(CommandError::from_service_error)?;
    if let Some(app) = options.app {
        emit::emit_job_progress(app, &job_id.0, 25, "Enumerating filesystem...");
    }

    let stats = match kind {
        domain::DataSourceKind::LogicalDirectory => {
            let fs =
                LogicalFsReader::open(&path, &ds.name).map_err(CommandError::from_service_error)?;
            file_service::enumerate_filesystem_with_root_name_and_cancel(
                conn,
                &ds.id,
                &fs,
                None,
                None::<&dyn Fn(u32)>,
                Some(options.cancel_token),
            )
            .map_err(CommandError::from_service_error)?
        }
        domain::DataSourceKind::E01 | domain::DataSourceKind::Raw => {
            // Load or create manifest for staging-based import
            let mut manifest =
                staging::StagingManifest::load(case_root, &ds.id.0).unwrap_or_else(|| {
                    staging::StagingManifest::create(
                        &ds.id.0,
                        source_path,
                        import_config.staging_kind().unwrap_or("Raw"),
                    )
                });

            // Probe once: detect partitions and filesystem candidates
            let mut probe_candidates: Vec<datasource_service::ImageFilesystemCandidate> =
                Vec::new();
            if manifest.partitions.is_empty() {
                let probe_started = Instant::now();
                let mut probe_reader: Box<dyn EvidenceReader> = if kind
                    == domain::DataSourceKind::E01
                {
                    Box::new(E01Reader::open(&path).map_err(CommandError::from_service_error)?)
                } else {
                    Box::new(RawImageReader::open(&path).map_err(CommandError::from_service_error)?)
                };
                let probe = datasource_service::detect_image_filesystem(&mut probe_reader)
                    .map_err(CommandError::from_service_error)?;
                emit_phase_profile(
                    options.app,
                    job_id,
                    case_id,
                    Some(&ds.id),
                    28,
                    format!(
                        "Probe complete: phase=probe elapsedMs={} partitions={} candidates={} rssMb={}",
                        elapsed_ms(probe_started.elapsed()),
                        probe.partitions.len(),
                        probe.candidates.len(),
                        import_analysis::current_rss_mb()
                    ),
                    options.cancel_token.load(Ordering::Relaxed),
                );

                // Store partition records in main DB
                file_service::store_data_source_partitions(conn, &ds.id, &probe.partitions)
                    .map_err(CommandError::from_service_error)?;

                let candidate_index_map =
                    datasource_service::assign_effective_partition_indices(&probe.candidates);

                let candidate_root_names = probe
                    .candidates
                    .iter()
                    .enumerate()
                    .map(|(i, candidate)| {
                        let index = datasource_service::effective_partition_index(
                            candidate,
                            i,
                            &candidate_index_map,
                        );
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
                        conn,
                        &ds.id,
                        partition.index,
                        &root_name,
                        status,
                    )
                    .map_err(CommandError::from_service_error)?;
                }

                // Build manifest entries for supported partitions
                for (i, candidate) in probe.candidates.iter().enumerate() {
                    let index = datasource_service::effective_partition_index(
                        candidate,
                        i,
                        &candidate_index_map,
                    );
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
                probe_candidates = probe.candidates;
                manifest
                    .save(case_root)
                    .map_err(CommandError::from_service_error)?;
            }

            if manifest.partitions.is_empty() {
                file_service::EnumerationStats {
                    file_count: 0,
                    dir_count: 0,
                    total_size: 0,
                    warnings: vec![],
                }
            } else {
                // Update partition statuses from staging DBs (for resume)
                for partition in &mut manifest.partitions {
                    let staging_db_path =
                        staging::staging_db_path(case_root, &ds.id.0, partition.index);
                    if staging_db_path.exists() {
                        if let Ok(staging_conn) =
                            staging::open_partition_staging(case_root, &ds.id.0, partition.index)
                        {
                            if let Ok(Some(status)) =
                                staging::get_staging_meta(&staging_conn, "status")
                            {
                                match status.as_str() {
                                    "done" => {
                                        partition.status = staging::PartitionStatus::Done;
                                        partition.file_count =
                                            staging::staging_db_row_count(&staging_conn)
                                                .unwrap_or(0);
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
                    .map_err(CommandError::from_service_error)?;

                if let Some(app) = options.app {
                    emit::emit_job_progress(app, &job_id.0, 30, "Building filesystem readers...");
                }

                // For resume: probe once to get candidates if we don't have them
                if probe_candidates.is_empty() {
                    let probe_started = Instant::now();
                    let mut probe_reader: Box<dyn EvidenceReader> = if kind
                        == domain::DataSourceKind::E01
                    {
                        Box::new(E01Reader::open(&path).map_err(CommandError::from_service_error)?)
                    } else {
                        Box::new(
                            RawImageReader::open(&path)
                                .map_err(CommandError::from_service_error)?,
                        )
                    };
                    let probe = datasource_service::detect_image_filesystem(&mut probe_reader)
                        .map_err(CommandError::from_service_error)?;
                    emit_phase_profile(
                        options.app,
                        job_id,
                        case_id,
                        Some(&ds.id),
                        30,
                        format!(
                            "Probe complete: phase=probe-resume elapsedMs={} candidates={} rssMb={}",
                            elapsed_ms(probe_started.elapsed()),
                            probe.candidates.len(),
                            import_analysis::current_rss_mb()
                        ),
                        options.cancel_token.load(Ordering::Relaxed),
                    );
                    probe_candidates = probe.candidates;
                }

                // Build work items for pending partitions — reuse probe results, no re-probe
                let build_started = Instant::now();
                let mut pending: Vec<crate::parallel_enum::PartitionWork> = Vec::new();
                let mut build_failures = Vec::new();
                for p in manifest.partitions.iter() {
                    if p.status == staging::PartitionStatus::Done {
                        continue;
                    }
                    let work = build_partition_work(
                        &path,
                        &kind,
                        p.index,
                        &p.name,
                        &p.fs_kind,
                        &probe_candidates,
                    );
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
                    options.app,
                    job_id,
                    case_id,
                    Some(&ds.id),
                    31,
                    format!(
                        "Reader build complete: phase=reader-build elapsedMs={} pending={} failures={} rssMb={}",
                        elapsed_ms(build_started.elapsed()),
                        pending.len(),
                        build_failures.len(),
                        import_analysis::current_rss_mb()
                    ),
                    options.cancel_token.load(Ordering::Relaxed),
                );
                if !build_failures.is_empty() {
                    counts.add_warnings(build_failures.len());
                    counts.add_failed(build_failures.len() as u32);
                    for (index, error) in &build_failures {
                        if let Some(partition) =
                            manifest.partitions.iter_mut().find(|p| p.index == *index)
                        {
                            partition.status = staging::PartitionStatus::Failed;
                            partition.error = Some(error.clone());
                        }
                    }
                    manifest
                        .save(case_root)
                        .map_err(CommandError::from_service_error)?;
                }

                if pending.is_empty() {
                    let done_count = manifest
                        .partitions
                        .iter()
                        .filter(|p| p.status == staging::PartitionStatus::Done)
                        .count();
                    if done_count == 0 && !build_failures.is_empty() {
                        job_repo
                            .update_outcome_counts(
                                job_id,
                                counts.warning_count,
                                counts.skipped_count,
                                counts.failed_count,
                                counts.is_partial(),
                            )
                            .map_err(CommandError::from_service_error)?;
                        return Err(CommandError::internal(
                            "No supported partitions could be enumerated",
                        ));
                    }
                    file_service::EnumerationStats {
                        file_count: manifest.partitions.iter().map(|p| p.file_count).sum(),
                        dir_count: manifest.partitions.iter().map(|p| p.dir_count).sum(),
                        total_size: manifest.partitions.iter().map(|p| p.total_size).sum(),
                        warnings: vec![],
                    }
                } else {
                    let max_workers =
                        crate::parallel_enum::resolve_worker_count(options.max_import_workers);
                    let ds_id_clone = ds.id.clone();
                    let app_ref = options.app;
                    let job_ref = job_id;
                    let case_root_clone = case_root.to_path_buf();
                    let total_partitions = manifest.partitions.len() as u32;

                    let enum_started = Instant::now();
                    let results = crate::parallel_enum::enumerate_partitions_parallel(
                        &case_root_clone,
                        &ds_id_clone,
                        pending,
                        max_workers,
                        Arc::clone(options.cancel_token),
                        &|partition_idx, pct, detail| {
                            if let Some(a) = app_ref {
                                let overall = 25 + (pct * 35 / 100);
                                emit::emit_job_progress(a, &job_ref.0, overall.min(60), detail);
                                emit::emit_partition_progress(
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
                    if options.cancel_token.load(Ordering::Relaxed) {
                        mark_import_cancelling(
                            &job_repo,
                            job_id,
                            "Cancellation acknowledged; draining enumeration workers",
                        );
                        emit_import_cancellation_state(
                            options.app,
                            job_id,
                            CancellationStateDto::Draining,
                            false,
                            "Cancellation acknowledged; draining enumeration workers",
                        );
                    }
                    emit_phase_profile(
                        options.app,
                        job_id,
                        case_id,
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
                        options.cancel_token.load(Ordering::Relaxed),
                    );

                    // Update manifest with results
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
                        counts.add_failed(failed_results as u32);
                    }
                    manifest.phase = staging::ImportPhase::Enumerating;
                    manifest
                        .save(case_root)
                        .map_err(CommandError::from_service_error)?;

                    if options.cancel_token.load(Ordering::Relaxed) {
                        job_repo
                            .update_outcome_counts(
                                job_id,
                                counts.warning_count,
                                counts.skipped_count.saturating_add(1),
                                counts.failed_count,
                                true,
                            )
                            .map_err(CommandError::from_service_error)?;
                        emit_import_cancellation_state(
                            options.app,
                            job_id,
                            CancellationStateDto::Acknowledged,
                            false,
                            "Import cancellation acknowledged after enumeration",
                        );
                        return Err(CommandError::internal("Import cancelled by user"));
                    }

                    let success_results = results
                        .iter()
                        .filter(|result| result.error.is_none())
                        .count();
                    if success_results == 0 {
                        counts.add_warnings(failed_results);
                        job_repo
                            .update_outcome_counts(
                                job_id,
                                counts.warning_count,
                                counts.skipped_count,
                                counts.failed_count,
                                counts.is_partial(),
                            )
                            .map_err(CommandError::from_service_error)?;
                        return Err(CommandError::internal(
                            "No supported partitions could be enumerated",
                        ));
                    }

                    // Merge staging → main DB
                    if let Some(a) = options.app {
                        emit::emit_job_progress(a, &job_id.0, 62, "Merging partitions...");
                    }
                    manifest.phase = staging::ImportPhase::Merging;
                    manifest
                        .save(case_root)
                        .map_err(CommandError::from_service_error)?;

                    let enum_merge_started = Instant::now();
                    let merged = staging::merge_all_staging_to_main(
                        conn,
                        case_root,
                        &ds.id.0,
                        &manifest,
                        Some(&|completed, total| {
                            if let Some(a) = options.app {
                                let pct = 62 + (completed as u32 * 8 / total as u32);
                                emit::emit_job_progress(
                                    a,
                                    &job_id.0,
                                    pct.min(70),
                                    &format!("Merged {}/{} partitions", completed, total),
                                );
                            }
                        }),
                    )
                    .map_err(CommandError::from_service_error)?;
                    let enum_merge_elapsed = enum_merge_started.elapsed();
                    emit_phase_profile(
                        options.app,
                        job_id,
                        case_id,
                        Some(&ds.id),
                        70,
                        format!(
                            "Partition merge complete: phase=enum-merge elapsedMs={} rows={} rowsPerSec={} rssMb={}",
                            elapsed_ms(enum_merge_elapsed),
                            merged,
                            rows_per_sec(merged, enum_merge_elapsed),
                            import_analysis::current_rss_mb()
                        ),
                        options.cancel_token.load(Ordering::Relaxed),
                    );

                    let total_files: u64 = results.iter().map(|r| r.file_count).sum();
                    let total_dirs: u64 = results.iter().map(|r| r.dir_count).sum();
                    let total_size: u64 = results.iter().map(|r| r.total_size).sum();
                    let warnings: Vec<String> = results
                        .iter()
                        .flat_map(|r| {
                            let mut warnings = r
                                .warnings
                                .iter()
                                .map(|warning| format!("Partition {}: {}", r.index, warning))
                                .collect::<Vec<_>>();
                            if let Some(error) = &r.error {
                                warnings.push(format!("Partition {}: {}", r.index, error));
                            }
                            warnings
                        })
                        .collect();

                    file_service::EnumerationStats {
                        file_count: total_files,
                        dir_count: total_dirs,
                        total_size,
                        warnings,
                    }
                }
            }
        }
    };
    counts.add_warnings(stats.warnings.len());
    emit_phase_profile(
        options.app,
        job_id,
        case_id,
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
        options.cancel_token.load(Ordering::Relaxed),
    );

    // Check for cancellation
    if options.cancel_token.load(Ordering::Relaxed) {
        mark_import_cancelling(
            &job_repo,
            job_id,
            "Cancellation acknowledged before post-import analysis",
        );
        emit_import_cancellation_state(
            options.app,
            job_id,
            CancellationStateDto::Acknowledged,
            false,
            "Cancellation acknowledged before post-import analysis",
        );
        emit_import_profile_progress(
            options.app,
            job_id,
            case_id,
            Some(&ds.id),
            70,
            "Cancellation acknowledged: phase=enumeration",
            true,
        );
        return Err(CommandError::internal("Import cancelled by user"));
    }

    job_repo
        .update_progress(job_id, 70, "Running post-import pipeline...")
        .map_err(CommandError::from_service_error)?;
    if let Some(app) = options.app {
        emit::emit_job_progress(app, &job_id.0, 70, "Running post-import pipeline...");
    }

    let post_import_db_path = case_root.join("app.db");
    let image_backed_source = import_config.is_image_backed();
    let analysis_mode = if image_backed_source {
        options.analysis_mode
    } else {
        match options.analysis_mode {
            import_analysis::ImportAnalysisMode::MetadataOnly => {
                import_analysis::ImportAnalysisMode::BudgetedContent
            }
            mode => mode,
        }
    };
    let post_import_started = Instant::now();
    let progress_adapter = |pct: u32, detail: &str| {
        emit_import_profile_progress(
            options.app,
            job_id,
            case_id,
            Some(&ds.id),
            pct,
            detail,
            options.cancel_token.load(Ordering::Relaxed),
        );
    };
    let (pipeline_msg, pipeline_counts) = import_analysis::run_post_import_pipeline_with_counts(
        import_analysis::PostImportPipelineOptions {
            case_root: case_root.to_path_buf(),
            db_path: post_import_db_path,
            case_id: case_id.0.clone(),
            data_source_id: ds.id.clone(),
            index_dir: index_dir.clone(),
            max_analysis_workers: options.max_analysis_workers,
            cancel_token: Arc::clone(options.cancel_token),
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
        counts.warning_count = counts
            .warning_count
            .saturating_add(service_counts.warning_count);
        counts.skipped_count = counts
            .skipped_count
            .saturating_add(service_counts.skipped_count);
        counts.failed_count = counts
            .failed_count
            .saturating_add(service_counts.failed_count);
        let cancellation_error = options.cancel_token.load(Ordering::Relaxed)
            || is_import_cancelled_message(&error.message);
        if cancellation_error {
            mark_import_cancelling(
                &job_repo,
                job_id,
                "Cancellation acknowledged during post-import analysis drain",
            );
            emit_import_cancellation_state(
                options.app,
                job_id,
                CancellationStateDto::Draining,
                false,
                "Cancellation acknowledged during post-import analysis drain",
            );
        }
        if cancellation_error {
            CommandError::internal("Import cancelled by user")
        } else {
            CommandError::from_service_error(error.message)
        }
    })?;
    let pipeline_counts = JobOutcomeCounts::from(pipeline_counts);
    let post_import_results = post_import_counts_from_message(&pipeline_msg);
    emit_phase_profile(
        options.app,
        job_id,
        case_id,
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
        options.cancel_token.load(Ordering::Relaxed),
    );
    counts.warning_count = counts
        .warning_count
        .saturating_add(pipeline_counts.warning_count);
    counts.skipped_count = counts
        .skipped_count
        .saturating_add(pipeline_counts.skipped_count);
    counts.failed_count = counts
        .failed_count
        .saturating_add(pipeline_counts.failed_count);
    job_repo
        .update_outcome_counts(
            job_id,
            counts.warning_count,
            counts.skipped_count,
            counts.failed_count,
            counts.is_partial(),
        )
        .map_err(CommandError::from_service_error)?;
    if let Some(app) = options.app {
        emit::emit_timeline_updated(app, stats.file_count + stats.dir_count);
        emit::emit_search_index_progress(app, 100, "Post-import indexing completed");
    }

    job_repo
        .update_progress(job_id, 95, "Finalizing...")
        .map_err(CommandError::from_service_error)?;
    if let Some(app) = options.app {
        emit::emit_job_progress(app, &job_id.0, 95, "Finalizing...");
    }

    if let Some(app) = options.app {
        match file_service::get_data_sources_real(conn, case_id)
            .map_err(CommandError::from_service_error)?
            .into_iter()
            .find(|source| source.id == ds.id.0)
        {
            Some(summary) => emit::emit_data_source_imported(app, &summary, &job_id.0),
            None => tracing::warn!(
                "Imported data source {} was not found in summary list for event emission",
                ds.id.0
            ),
        }
    }
    emit_phase_profile(
        options.app,
        job_id,
        case_id,
        Some(&ds.id),
        99,
        format!(
            "Import profile complete: phase=total elapsedMs={} rssMb={}",
            elapsed_ms(import_started.elapsed()),
            import_analysis::current_rss_mb()
        ),
        options.cancel_token.load(Ordering::Relaxed),
    );

    let mut msg = format!(
        "Imported {}: {} files, {} dirs",
        source_name, stats.file_count, stats.dir_count
    );
    if !pipeline_msg.is_empty() {
        msg.push_str(". ");
        msg.push_str(&pipeline_msg);
    }

    // Record investigation step for provenance
    let import_duration_ms = import_started.elapsed().as_millis() as u32;
    let params_json = serde_json::json!({
        "sourcePath": source_path,
        "sourceName": source_name,
        "kind": format!("{:?}", kind),
        "filesEnumerated": stats.file_count,
        "dirsEnumerated": stats.dir_count,
    })
    .to_string();
    let _ = step_recorder::record_step(
        conn,
        &case_id.0,
        "import",
        &params_json,
        import_duration_ms,
        true,
        None,
    );

    Ok((msg, counts))
}

// ---------------------------------------------------------------------------
// Cancellation helpers
// ---------------------------------------------------------------------------

pub(crate) fn emit_import_cancellation_state(
    app: Option<&AppHandle>,
    job_id: &domain::JobId,
    state: CancellationStateDto,
    safe_to_close: bool,
    detail: &str,
) {
    if let Some(app) = app {
        emit::emit_job_cancellation(
            app,
            &job_cancellation_dto(&job_id.0, state, safe_to_close, detail),
        );
    }
}

pub(crate) fn job_cancellation_dto(
    job_id: &str,
    state: CancellationStateDto,
    safe_to_close: bool,
    detail: &str,
) -> transport::dto::JobCancellationDto {
    let now = chrono::Utc::now().to_rfc3339();
    transport::dto::JobCancellationDto {
        job_id: job_id.to_string(),
        requested_at: Some(now.clone()),
        acknowledged_at: matches!(
            state,
            CancellationStateDto::Acknowledged
                | CancellationStateDto::Draining
                | CancellationStateDto::Cancelled
                | CancellationStateDto::TimedOut
        )
        .then_some(now),
        state,
        safe_to_close,
        detail: detail.to_string(),
    }
}

pub(crate) fn mark_import_cancelling(job_repo: &JobRepo<'_>, job_id: &domain::JobId, detail: &str) {
    if let Err(error) = job_repo.mark_cancelling(job_id, detail) {
        tracing::warn!("Failed to mark job {} as cancelling: {}", job_id.0, error);
    }
}

pub(crate) fn is_import_cancelled_message(message: &str) -> bool {
    message.to_ascii_lowercase().contains("cancel")
}

// ---------------------------------------------------------------------------
// Progress profile helpers
// ---------------------------------------------------------------------------

pub(crate) fn emit_phase_profile(
    app: Option<&AppHandle>,
    job_id: &domain::JobId,
    case_id: &domain::CaseId,
    data_source_id: Option<&domain::DataSourceId>,
    progress: u32,
    detail: String,
    cancel_requested: bool,
) {
    emit_import_profile_progress(
        app,
        job_id,
        case_id,
        data_source_id,
        progress,
        &detail,
        cancel_requested,
    );
}

pub(crate) fn emit_import_profile_progress(
    app: Option<&AppHandle>,
    job_id: &domain::JobId,
    case_id: &domain::CaseId,
    data_source_id: Option<&domain::DataSourceId>,
    progress: u32,
    detail: &str,
    cancel_requested: bool,
) {
    tracing::info!("Import profile for {}: {}", job_id.0, detail);
    #[cfg(test)]
    eprintln!("[import-profile] {}% {}", progress.min(99), detail);
    if let Some(app) = app {
        let phase_progress = import_phase_progress_from_profile(
            job_id,
            case_id,
            data_source_id,
            progress,
            detail,
            cancel_requested,
        );
        emit::emit_import_phase_progress(app, &phase_progress);
        for result in &phase_progress.partial_results {
            emit::emit_import_partial_result(app, result);
        }
        for status in cache_statuses_from_profile(data_source_id, detail) {
            emit::emit_cache_index_status(app, &status);
        }
        emit::emit_job_progress(app, &job_id.0, progress.min(99), detail);
    }
}

pub(crate) fn import_phase_progress_from_profile(
    job_id: &domain::JobId,
    case_id: &domain::CaseId,
    data_source_id: Option<&domain::DataSourceId>,
    progress: u32,
    detail: &str,
    cancel_requested: bool,
) -> ImportPhaseProgressDto {
    ImportPhaseProgressDto {
        job_id: job_id.0.clone(),
        case_id: case_id.0.clone(),
        data_source_id: data_source_id.map(|id| id.0.clone()),
        phase: import_phase_from_profile(detail, progress),
        state: import_phase_state_from_profile(detail, cancel_requested),
        percent: progress.min(99),
        detail: detail.to_string(),
        metrics: import_phase_metrics_from_profile(detail),
        partial_results: partial_results_from_profile(data_source_id, detail),
        cancellable: progress < 99,
        cancel_requested,
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PostImportResultCounts {
    pub(crate) timeline_events: u64,
    pub(crate) artifact_count: u64,
    pub(crate) indexed_count: u64,
}

pub(crate) fn partial_results_from_profile(
    data_source_id: Option<&domain::DataSourceId>,
    detail: &str,
) -> Vec<PartialResultDto> {
    let Some(scope_id) = data_source_id.map(|id| id.0.as_str()) else {
        return Vec::new();
    };
    let lower = detail.to_ascii_lowercase();
    if lower.contains("layout changed") && lower.contains("reinitializing") {
        return analysis_slice_results(scope_id, 0, None, ResultFreshnessDto::Invalidated);
    }
    if lower.contains("already merged") {
        return analysis_slice_results(scope_id, 0, None, ResultFreshnessDto::Stale);
    }

    match profile_value(detail, "phase").as_deref() {
        Some("enum-merge") => {
            let rows = profile_u64(detail, "rows").unwrap_or(0);
            let freshness = if lower.contains("complete") || lower.contains("ready") {
                ResultFreshnessDto::Ready
            } else {
                ResultFreshnessDto::Partial
            };
            vec![
                partial_result(
                    PartialResultKindDto::FileRows,
                    scope_id,
                    rows,
                    Some(rows),
                    "files:rows",
                    freshness.clone(),
                ),
                partial_result(
                    PartialResultKindDto::FileTree,
                    scope_id,
                    rows,
                    Some(rows),
                    "files:tree",
                    freshness,
                ),
            ]
        }
        Some("analysis") => {
            let indexed = profile_u64(detail, "indexed").unwrap_or(0);
            let total = profile_u64(detail, "files")
                .or_else(|| rows_from_profile(detail).1)
                .or_else(|| profile_u64(detail, "queuedTasks"));
            vec![partial_result(
                PartialResultKindDto::SearchIndex,
                scope_id,
                indexed,
                total,
                "search:index",
                ResultFreshnessDto::Partial,
            )]
        }
        Some("post-import-skip") => vec![
            partial_result(
                PartialResultKindDto::TimelineEvents,
                scope_id,
                0,
                None,
                "timeline:events",
                ResultFreshnessDto::Deferred,
            ),
            partial_result(
                PartialResultKindDto::ArtifactFamily,
                scope_id,
                0,
                None,
                "artifacts:family",
                ResultFreshnessDto::Deferred,
            ),
            partial_result(
                PartialResultKindDto::SearchIndex,
                scope_id,
                0,
                None,
                "search:index",
                ResultFreshnessDto::Deferred,
            ),
        ],
        Some("post-import") => {
            let counts = post_import_counts_from_profile(detail);
            analysis_ready_results(scope_id, counts)
        }
        _ => Vec::new(),
    }
}

fn analysis_ready_results(scope_id: &str, counts: PostImportResultCounts) -> Vec<PartialResultDto> {
    vec![
        partial_result(
            PartialResultKindDto::TimelineEvents,
            scope_id,
            counts.timeline_events,
            Some(counts.timeline_events),
            "timeline:events",
            ResultFreshnessDto::Ready,
        ),
        partial_result(
            PartialResultKindDto::ArtifactFamily,
            scope_id,
            counts.artifact_count,
            Some(counts.artifact_count),
            "artifacts:family",
            ResultFreshnessDto::Ready,
        ),
        partial_result(
            PartialResultKindDto::SearchIndex,
            scope_id,
            counts.indexed_count,
            Some(counts.indexed_count),
            "search:index",
            ResultFreshnessDto::Ready,
        ),
    ]
}

pub(crate) fn cache_statuses_from_profile(
    data_source_id: Option<&domain::DataSourceId>,
    detail: &str,
) -> Vec<IndexCacheStatusDto> {
    let Some(scope_id) = data_source_id.map(|id| id.0.as_str()) else {
        return Vec::new();
    };
    let lower = detail.to_ascii_lowercase();
    if lower.contains("layout changed") && lower.contains("reinitializing") {
        return analysis_cache_statuses(
            scope_id,
            "invalidated",
            0,
            None,
            Some("Analysis staging layout changed; derived caches invalidated"),
        );
    }
    if lower.contains("already merged") {
        return analysis_cache_statuses(
            scope_id,
            "reused",
            0,
            None,
            Some("Previously merged analysis output reused"),
        );
    }
    if lower.contains("merging analysis staging dbs") {
        return analysis_cache_statuses(
            scope_id,
            "stale",
            0,
            None,
            Some("Worker output is being merged; existing derived caches may be stale"),
        );
    }

    match profile_value(detail, "phase").as_deref() {
        Some("analysis-start") => {
            let total = profile_u64(detail, "pendingTasks");
            analysis_cache_statuses(
                scope_id,
                "warming",
                0,
                total,
                Some("Post-import analysis queued; derived caches warming"),
            )
        }
        Some("analysis") => {
            let indexed = profile_u64(detail, "indexed").unwrap_or(0);
            let total = profile_u64(detail, "files")
                .or_else(|| rows_from_profile(detail).1)
                .or_else(|| profile_u64(detail, "queuedTasks"));
            analysis_cache_statuses(
                scope_id,
                "warming",
                indexed,
                total,
                Some("Post-import analysis running; derived caches warming"),
            )
        }
        Some("post-import-skip") => analysis_cache_statuses(
            scope_id,
            "deferred",
            0,
            None,
            Some("Metadata-only import deferred timeline, artifact, and search index caches"),
        ),
        Some("post-import") => {
            let counts = post_import_counts_from_profile(detail);
            analysis_cache_ready_statuses(scope_id, counts)
        }
        _ => Vec::new(),
    }
}

fn analysis_cache_ready_statuses(
    scope_id: &str,
    counts: PostImportResultCounts,
) -> Vec<IndexCacheStatusDto> {
    vec![
        cache_status(
            "timeline:events",
            scope_id,
            "ready",
            counts.timeline_events,
            Some(counts.timeline_events),
            Some("Timeline projection ready"),
        ),
        cache_status(
            "artifacts:family",
            scope_id,
            "ready",
            counts.artifact_count,
            Some(counts.artifact_count),
            Some("Artifact analysis cache ready"),
        ),
        cache_status(
            "search:index",
            scope_id,
            "ready",
            counts.indexed_count,
            Some(counts.indexed_count),
            Some("Search index ready"),
        ),
    ]
}

fn analysis_cache_statuses(
    scope_id: &str,
    state: &str,
    indexed_count: u64,
    total_count: Option<u64>,
    message: Option<&str>,
) -> Vec<IndexCacheStatusDto> {
    vec![
        cache_status(
            "timeline:events",
            scope_id,
            state,
            indexed_count,
            total_count,
            message,
        ),
        cache_status(
            "artifacts:family",
            scope_id,
            state,
            indexed_count,
            total_count,
            message,
        ),
        cache_status(
            "search:index",
            scope_id,
            state,
            indexed_count,
            total_count,
            message,
        ),
    ]
}

fn cache_status(
    key_prefix: &str,
    scope_id: &str,
    state: &str,
    indexed_count: u64,
    total_count: Option<u64>,
    message: Option<&str>,
) -> IndexCacheStatusDto {
    IndexCacheStatusDto {
        cache_key: format!("{key_prefix}:{scope_id}"),
        state: state.to_string(),
        indexed_count,
        total_count,
        updated_at: chrono::Utc::now().to_rfc3339(),
        message: message.map(str::to_string),
    }
}

fn analysis_slice_results(
    scope_id: &str,
    ready_count: u64,
    total_estimate: Option<u64>,
    freshness: ResultFreshnessDto,
) -> Vec<PartialResultDto> {
    vec![
        partial_result(
            PartialResultKindDto::TimelineEvents,
            scope_id,
            ready_count,
            total_estimate,
            "timeline:events",
            freshness.clone(),
        ),
        partial_result(
            PartialResultKindDto::ArtifactFamily,
            scope_id,
            ready_count,
            total_estimate,
            "artifacts:family",
            freshness.clone(),
        ),
        partial_result(
            PartialResultKindDto::SearchIndex,
            scope_id,
            ready_count,
            total_estimate,
            "search:index",
            freshness,
        ),
    ]
}

fn partial_result(
    kind: PartialResultKindDto,
    scope_id: &str,
    ready_count: u64,
    total_estimate: Option<u64>,
    key_prefix: &str,
    freshness: ResultFreshnessDto,
) -> PartialResultDto {
    PartialResultDto {
        kind,
        scope_id: scope_id.to_string(),
        ready_count,
        total_estimate,
        query_key: format!("{key_prefix}:{scope_id}"),
        freshness,
    }
}

pub(crate) fn post_import_counts_from_profile(detail: &str) -> PostImportResultCounts {
    PostImportResultCounts {
        timeline_events: profile_u64(detail, "timeline").unwrap_or(0),
        artifact_count: profile_u64(detail, "artifacts").unwrap_or(0),
        indexed_count: profile_u64(detail, "indexed").unwrap_or(0),
    }
}

pub(crate) fn post_import_counts_from_message(message: &str) -> PostImportResultCounts {
    let normalized = message.replace([':', '.', ','], " ");
    let parts: Vec<&str> = normalized.split_whitespace().collect();
    PostImportResultCounts {
        timeline_events: value_after_label(&parts, "Timeline").unwrap_or(0),
        artifact_count: value_after_label(&parts, "Artifacts").unwrap_or(0),
        indexed_count: value_after_label(&parts, "Index").unwrap_or(0),
    }
}

fn value_after_label(parts: &[&str], label: &str) -> Option<u64> {
    parts.windows(2).find_map(|window| {
        (window[0] == label)
            .then(|| window[1].parse::<u64>().ok())
            .flatten()
    })
}

fn import_phase_from_profile(detail: &str, progress: u32) -> ImportPhaseDto {
    match profile_value(detail, "phase").as_deref() {
        Some("attach") => ImportPhaseDto::Attach,
        Some("probe") | Some("probe-resume") | Some("reader-build") => ImportPhaseDto::Probe,
        Some("enumeration") => ImportPhaseDto::Enumerate,
        Some("enum-merge") => ImportPhaseDto::MergeEnumeration,
        Some("analysis-start") | Some("analysis") => ImportPhaseDto::Analyze,
        Some("analysis-merge") => ImportPhaseDto::MergeAnalysis,
        Some("post-import") | Some("post-import-skip") | Some("total") => ImportPhaseDto::Finalize,
        _ if progress < 25 => ImportPhaseDto::Attach,
        _ if progress < 70 => ImportPhaseDto::Enumerate,
        _ if progress < 84 => ImportPhaseDto::Analyze,
        _ if progress < 95 => ImportPhaseDto::MergeAnalysis,
        _ => ImportPhaseDto::Finalize,
    }
}

fn import_phase_state_from_profile(detail: &str, cancel_requested: bool) -> ImportPhaseStateDto {
    if cancel_requested {
        return ImportPhaseStateDto::Cancelling;
    }
    let lower = detail.to_ascii_lowercase();
    if lower.contains("cancel") {
        ImportPhaseStateDto::Cancelling
    } else if lower.contains("skipped")
        || profile_value(detail, "phase").as_deref() == Some("post-import-skip")
    {
        ImportPhaseStateDto::Skipped
    } else if lower.contains("complete")
        || lower.contains("ready")
        || lower.contains("already merged")
    {
        ImportPhaseStateDto::Completed
    } else if lower.contains("failed") || lower.contains("hard limit exceeded") {
        ImportPhaseStateDto::Failed
    } else {
        ImportPhaseStateDto::Running
    }
}

fn import_phase_metrics_from_profile(detail: &str) -> ImportPhaseMetricsDto {
    let (rows_processed, rows_total) = rows_from_profile(detail);
    let bytes_processed = profile_u64(detail, "bytes")
        .or_else(|| profile_u64(detail, "dataMb").map(|mb| mb.saturating_mul(1024 * 1024)))
        .unwrap_or(0);
    ImportPhaseMetricsDto {
        elapsed_ms: profile_u64(detail, "elapsedMs").unwrap_or(0),
        rss_mb: profile_u64(detail, "rssMb").unwrap_or(0),
        workers: profile_u64(detail, "workers")
            .or_else(|| profile_nonzero_u64(detail, "activeWorkers"))
            .or_else(|| profile_nonzero_u64(detail, "active"))
            .or_else(|| profile_u64(detail, "workerBudget"))
            .or_else(|| profile_u64(detail, "activeWorkers"))
            .or_else(|| profile_u64(detail, "active"))
            .unwrap_or(0) as u32,
        rows_processed,
        rows_total,
        rows_per_sec: profile_f64(detail, "rowsPerSec"),
        bytes_processed,
        bytes_total: profile_u64(detail, "bytesTotal"),
        mb_per_sec: profile_f64(detail, "mbPerSec"),
        warnings: profile_u64(detail, "warnings").unwrap_or(0) as u32,
        skipped: profile_u64(detail, "skipped").unwrap_or(0) as u32,
        failed: profile_u64(detail, "failed")
            .or_else(|| profile_u64(detail, "failures"))
            .unwrap_or(0) as u32,
    }
}

fn rows_from_profile(detail: &str) -> (u64, Option<u64>) {
    if let Some(processed) = profile_value(detail, "processed") {
        if let Some((done, total)) = processed.split_once('/') {
            return (done.parse::<u64>().unwrap_or(0), total.parse::<u64>().ok());
        }
        if let Ok(rows) = processed.parse::<u64>() {
            return (rows, profile_u64(detail, "files"));
        }
    }
    let rows = profile_u64(detail, "rows").unwrap_or(0);
    (
        rows,
        profile_u64(detail, "files").or_else(|| profile_u64(detail, "pendingTasks")),
    )
}

fn profile_u64(detail: &str, key: &str) -> Option<u64> {
    profile_value(detail, key).and_then(|value| value.parse::<u64>().ok())
}

fn profile_nonzero_u64(detail: &str, key: &str) -> Option<u64> {
    profile_u64(detail, key).filter(|value| *value > 0)
}

fn profile_f64(detail: &str, key: &str) -> Option<f64> {
    profile_value(detail, key).and_then(|value| value.parse::<f64>().ok())
}

fn profile_value(detail: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    detail.split_whitespace().find_map(|part| {
        part.strip_prefix(&prefix)
            .map(|value| value.trim_end_matches([',', ';']).to_string())
    })
}

pub(crate) fn elapsed_ms(duration: Duration) -> u128 {
    duration.as_millis()
}

pub(crate) fn rows_per_sec(rows: u64, duration: Duration) -> u64 {
    let secs = duration.as_secs_f64();
    if secs <= 0.0 {
        rows
    } else {
        (rows as f64 / secs).round() as u64
    }
}

pub(crate) fn bytes_to_mb(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

pub(crate) fn mb_per_sec(bytes: u64, duration: Duration) -> u64 {
    let secs = duration.as_secs_f64();
    if secs <= 0.0 {
        bytes_to_mb(bytes)
    } else {
        ((bytes as f64 / (1024.0 * 1024.0)) / secs).round() as u64
    }
}
