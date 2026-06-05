//! Import pipeline orchestration.
//!
//! Handles the complete import workflow:
//! 1. Classify data source type (E01/RAW/logical)
//! 2. Create background job
//! 3. Enumerate filesystem
//! 4. Run post-import pipeline (timeline/artifacts/indexing)
//! 5. Report progress via events

use app_services::{
    datasource_service::{self, ImageFilesystemKind},
    file_service, import_analysis, import_precheck, staging,
};
use domain::DataSourceKind;
use evidence_core::{EvidenceReader, FileSystemReader, LogicalFsReader, RawImageReader};
use image_e01::E01Reader;
use persistence_sqlite::repositories::job_repo::JobRepo;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, State};
use transport::{
    commands::{AppSettingsDto, ImportDataSourceRequest},
    dto::{
        CancellationStateDto, ImportPhaseDto, ImportPhaseMetricsDto, ImportPhaseProgressDto,
        ImportPhaseStateDto, IndexCacheStatusDto, JobCancellationDto, PartialResultDto,
        PartialResultKindDto, ResultFreshnessDto,
    },
    CommandError,
};

use crate::events::event_bridge;
use crate::state::AppState;

#[derive(Debug, Clone, Default)]
pub struct JobOutcomeCounts {
    pub warning_count: u32,
    pub skipped_count: u32,
    pub failed_count: u32,
}

#[derive(Clone, Copy)]
pub struct ImportJobOptions<'a> {
    pub app: Option<&'a AppHandle>,
    pub cancel_token: &'a Arc<AtomicBool>,
    pub max_import_workers: Option<usize>,
    pub max_analysis_workers: Option<usize>,
    pub analysis_mode: import_analysis::ImportAnalysisMode,
}

struct BackgroundImportJob {
    db_path: PathBuf,
    case_id: domain::CaseId,
    case_root: PathBuf,
    source_path: String,
    job_id: domain::JobId,
    max_import_workers: Option<usize>,
    max_analysis_workers: Option<usize>,
    analysis_mode: import_analysis::ImportAnalysisMode,
}

impl JobOutcomeCounts {
    fn add_warnings(&mut self, count: usize) {
        self.warning_count = self.warning_count.saturating_add(count as u32);
    }

    #[allow(dead_code)]
    fn add_skipped(&mut self, count: u32) {
        self.skipped_count = self.skipped_count.saturating_add(count);
    }

    fn add_failed(&mut self, count: u32) {
        self.failed_count = self.failed_count.saturating_add(count);
    }

    fn is_partial(&self) -> bool {
        self.warning_count > 0 || self.skipped_count > 0 || self.failed_count > 0
    }
}

impl From<import_analysis::JobOutcomeCounts> for JobOutcomeCounts {
    fn from(counts: import_analysis::JobOutcomeCounts) -> Self {
        Self {
            warning_count: counts.warning_count,
            skipped_count: counts.skipped_count,
            failed_count: counts.failed_count,
        }
    }
}

/// Enumerate a filesystem within a partition, handling placeholder root replacement.
#[allow(dead_code)]
fn enumerate_partition_with_fs(
    conn: &rusqlite::Connection,
    data_source_id: &domain::DataSourceId,
    fs: &dyn evidence_core::FileSystemReader,
    root_name: &str,
    placeholder_roots: &std::collections::HashMap<usize, domain::FileEntryId>,
    candidate: &app_services::datasource_service::ImageFilesystemCandidate,
    progress_cb: Option<&dyn Fn(u32)>,
) -> persistence_sqlite::DbResult<file_service::EnumerationStats> {
    if let Some(partition_index) = candidate.partition_index {
        if let Some(placeholder_id) = placeholder_roots.get(&partition_index) {
            return file_service::replace_placeholder_root_with_real(
                conn,
                placeholder_id,
                fs,
                Some(root_name),
                progress_cb,
            );
        }
    }
    file_service::enumerate_filesystem_with_root_name(
        conn,
        data_source_id,
        fs,
        Some(root_name),
        progress_cb,
    )
}

/// Tauri command: Import a data source into the current case.
#[tauri::command]
pub async fn import_data_source(
    state: State<'_, AppState>,
    app: AppHandle,
    request: ImportDataSourceRequest,
) -> Result<String, CommandError> {
    let import_config = prepare_import_config(&request)?;
    let app_state = state.inner().clone();
    let source_path = import_config.source_path_display.clone();
    let app_clone = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_import_settings(&app_state.app_settings_path);
        let guard = app_state
            .active_case
            .lock()
            .map_err(|e| CommandError::from_lock_error("Case", e))?;
        let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
        let _job_id_str = schedule_import_for_active_case(
            active,
            &source_path,
            Some(&app_clone),
            &app_state.task_manager,
            settings.max_import_workers,
            settings.max_analysis_workers,
            import_analysis_mode_from_settings(&settings.import_analysis_mode),
        )?;
        Ok(format!(
            "Import started for {}. Watch the Jobs panel for progress.",
            source_path
        ))
    })
    .await
    .map_err(CommandError::from_join_error)?
}

fn load_import_settings(path: &std::path::Path) -> AppSettingsDto {
    match std::fs::read_to_string(path) {
        Ok(raw) => match serde_json::from_str::<AppSettingsDto>(&raw) {
            Ok(settings) => {
                if let Err(error) = settings.validate() {
                    tracing::warn!(
                        "Ignoring invalid app settings at {}: {}",
                        path.display(),
                        error
                    );
                    AppSettingsDto::default()
                } else {
                    settings
                }
            }
            Err(error) => {
                tracing::warn!(
                    "Ignoring unreadable app settings at {}: {}",
                    path.display(),
                    error
                );
                AppSettingsDto::default()
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => AppSettingsDto::default(),
        Err(error) => {
            tracing::warn!(
                "Ignoring app settings read error at {}: {}",
                path.display(),
                error
            );
            AppSettingsDto::default()
        }
    }
}

/// Tauri command: Cancel an in-progress import job.
#[tauri::command]
pub async fn cancel_import(
    state: State<'_, AppState>,
    app: AppHandle,
    job_id: String,
) -> Result<String, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if app_state.task_manager.cancel(&job_id) {
            tracing::info!("Cancel requested for job {}", job_id);
            if let Ok(guard) = app_state.active_case.lock() {
                if let Some(active) = guard.as_ref() {
                    match persistence_sqlite::open_or_create(&active.db_path()) {
                        Ok(conn) => {
                            let repo = JobRepo::new(&conn);
                            if let Err(error) = repo.mark_cancelling(
                                &domain::JobId(job_id.clone()),
                                "Cancel requested by user",
                            ) {
                                tracing::warn!(
                                    "Failed to mark job {} as cancelling: {}",
                                    job_id,
                                    error
                                );
                            }
                        }
                        Err(error) => tracing::warn!(
                            "Failed to open case DB while cancelling job {}: {}",
                            job_id,
                            error
                        ),
                    }
                }
            }
            event_bridge::emit_job_cancelled(&app, &job_id, "Cancel requested by user");
            event_bridge::emit_job_cancellation(
                &app,
                &job_cancellation_dto(
                    &job_id,
                    CancellationStateDto::Requested,
                    false,
                    "Cancel requested by user",
                ),
            );
            Ok("Cancel requested".to_string())
        } else {
            Err(CommandError::not_found("Job"))
        }
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Schedule an import job for the active case.
pub fn schedule_import_for_active_case(
    active: &app_services::active_case::ActiveCase,
    source_path: &str,
    app: Option<&AppHandle>,
    task_manager: &crate::state::TaskManager,
    max_import_workers: Option<usize>,
    max_analysis_workers: Option<usize>,
    analysis_mode: import_analysis::ImportAnalysisMode,
) -> Result<String, CommandError> {
    let case_id = active.meta.id.clone();
    let case_root = active.case_root.clone();
    let db_path = active.db_path();
    let import_config = import_precheck::prepare_import_source_config_from_path(source_path)
        .map_err(import_config_error_to_command_error)?;
    let source_name = import_config.source_name.clone();

    let conn =
        persistence_sqlite::open_or_create(&db_path).map_err(CommandError::from_service_error)?;
    let job_repo = JobRepo::new(&conn);
    let job_id = job_repo
        .create(&case_id.0, "Import data source")
        .map_err(CommandError::from_service_error)?;
    let job_id_str = job_id.0.clone();
    if let Some(app) = app {
        event_bridge::emit_job_created(app, &job_id_str, "Import data source");
    }
    job_repo
        .update_progress(&job_id, 1, &format!("Queued import for {source_name}"))
        .map_err(CommandError::from_service_error)?;

    let source_path = source_path.to_string();
    let app_handle = app.cloned();
    let cancel_token = Arc::new(AtomicBool::new(false));
    let cancel_token_clone = cancel_token.clone();

    let _job_id_clone = job_id.clone();
    let background_job = BackgroundImportJob {
        db_path,
        case_id,
        case_root,
        source_path,
        job_id,
        max_import_workers,
        max_analysis_workers,
        analysis_mode,
    };
    let handle = std::thread::spawn(move || {
        run_background_import_job(background_job, app_handle.as_ref(), cancel_token_clone)
            .map_err(|e| e.message)
    });

    // Register with TaskManager using the cancel token
    task_manager.register_with_token(job_id_str.clone(), handle, cancel_token);

    Ok(job_id_str)
}

/// Run the background import job in a separate thread.
fn run_background_import_job(
    job: BackgroundImportJob,
    app: Option<&AppHandle>,
    cancel_token: Arc<AtomicBool>,
) -> Result<(), CommandError> {
    let conn = persistence_sqlite::open_or_create(&job.db_path)
        .map_err(CommandError::from_service_error)?;
    let job_repo = JobRepo::new(&conn);

    if let Some(app) = app {
        event_bridge::emit_job_started(app, &job.job_id.0, "Import started");
        event_bridge::emit_job_progress(app, &job.job_id.0, 5, "Import started");
    }

    // Check for cancellation before starting
    if cancel_token.load(Ordering::Relaxed) {
        let msg = "Import cancelled by user";
        if let Err(e) = job_repo.cancel(&job.job_id, msg) {
            tracing::error!("Failed to mark job {} as cancelled: {}", job.job_id.0, e);
        }
        if let Some(app) = app {
            event_bridge::emit_job_cancelled(app, &job.job_id.0, msg);
            event_bridge::emit_job_cancellation(
                app,
                &job_cancellation_dto(&job.job_id.0, CancellationStateDto::Cancelled, true, msg),
            );
        }
        return Ok(());
    }

    let options = ImportJobOptions {
        app,
        cancel_token: &cancel_token,
        max_import_workers: job.max_import_workers,
        max_analysis_workers: job.max_analysis_workers,
        analysis_mode: job.analysis_mode,
    };
    match execute_import_job(
        &conn,
        &job.case_id,
        &job.case_root,
        &job.source_path,
        &job.job_id,
        options,
    ) {
        Ok(message) => {
            job_repo
                .complete(&job.job_id, &message)
                .map_err(CommandError::from_service_error)?;
            if let Some(app) = app {
                event_bridge::emit_job_completed(app, &job.job_id.0, &message);
            }
            Ok(())
        }
        Err(error) => {
            if is_import_cancelled_message(&error.message) {
                if let Err(e) = job_repo.cancel(&job.job_id, &error.message) {
                    tracing::error!("Failed to mark job {} as cancelled: {}", job.job_id.0, e);
                }
                if let Some(app) = app {
                    event_bridge::emit_job_cancelled(app, &job.job_id.0, &error.message);
                    event_bridge::emit_job_cancellation(
                        app,
                        &job_cancellation_dto(
                            &job.job_id.0,
                            CancellationStateDto::Cancelled,
                            true,
                            &error.message,
                        ),
                    );
                }
                Ok(())
            } else {
                if let Err(e) = job_repo.fail(&job.job_id, &error.message) {
                    tracing::error!("Failed to mark job {} as failed: {}", job.job_id.0, e);
                }
                if let Some(app) = app {
                    event_bridge::emit_job_failed(app, &job.job_id.0, &error.message);
                }
                Err(error)
            }
        }
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

fn execute_import_job_with_counts(
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
        event_bridge::emit_job_progress(app, &job_id.0, 10, &format!("Attaching {source_name}"));
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
        event_bridge::emit_job_progress(app, &job_id.0, 25, "Enumerating filesystem...");
    }

    let stats = match kind {
        DataSourceKind::LogicalDirectory => {
            let fs =
                LogicalFsReader::open(&path, &ds.name).map_err(CommandError::from_service_error)?;
            file_service::enumerate_filesystem(conn, &ds.id, &fs)
                .map_err(CommandError::from_service_error)?
        }
        DataSourceKind::E01 | DataSourceKind::Raw => {
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
                let mut probe_reader: Box<dyn EvidenceReader> = if kind == DataSourceKind::E01 {
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

                // Build manifest entries for supported partitions
                for candidate in &probe.candidates {
                    let name = format_partition_root_name(candidate);
                    manifest.partitions.push(staging::PartitionEntry {
                        index: candidate.partition_index.unwrap_or(0),
                        name,
                        fs_kind: format!("{:?}", candidate.kind),
                        staging_db: format!(
                            "enum_partition_{}.db",
                            candidate.partition_index.unwrap_or(0)
                        ),
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
                    event_bridge::emit_job_progress(
                        app,
                        &job_id.0,
                        30,
                        "Building filesystem readers...",
                    );
                }

                // For resume: probe once to get candidates if we don't have them
                if probe_candidates.is_empty() {
                    let probe_started = Instant::now();
                    let mut probe_reader: Box<dyn EvidenceReader> = if kind == DataSourceKind::E01 {
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
                let mut pending: Vec<app_services::parallel_enum::PartitionWork> = Vec::new();
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
                    let max_workers = app_services::parallel_enum::resolve_worker_count(
                        options.max_import_workers,
                    );
                    let ds_id_clone = ds.id.clone();
                    let app_ref = options.app;
                    let job_ref = job_id;
                    let case_root_clone = case_root.to_path_buf();
                    let total_partitions = manifest.partitions.len() as u32;

                    let enum_started = Instant::now();
                    let results = app_services::parallel_enum::enumerate_partitions_parallel(
                        &case_root_clone,
                        &ds_id_clone,
                        pending,
                        max_workers,
                        Arc::clone(options.cancel_token),
                        &|partition_idx, pct, detail| {
                            if let Some(a) = app_ref {
                                let overall = 25 + (pct * 35 / 100);
                                event_bridge::emit_job_progress(
                                    a,
                                    &job_ref.0,
                                    overall.min(60),
                                    detail,
                                );
                                event_bridge::emit_partition_progress(
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
                        event_bridge::emit_job_progress(a, &job_id.0, 62, "Merging partitions...");
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
                                event_bridge::emit_job_progress(
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
        event_bridge::emit_job_progress(app, &job_id.0, 70, "Running post-import pipeline...");
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
        event_bridge::emit_timeline_updated(app, stats.file_count + stats.dir_count);
        event_bridge::emit_search_index_progress(app, 100, "Post-import indexing completed");
    }

    job_repo
        .update_progress(job_id, 95, "Finalizing...")
        .map_err(CommandError::from_service_error)?;
    if let Some(app) = options.app {
        event_bridge::emit_job_progress(app, &job_id.0, 95, "Finalizing...");
    }

    if let Some(app) = options.app {
        match app_services::file_service::get_data_sources_real(conn, case_id)
            .map_err(CommandError::from_service_error)?
            .into_iter()
            .find(|source| source.id == ds.id.0)
        {
            Some(summary) => event_bridge::emit_data_source_imported(app, &summary, &job_id.0),
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

    Ok((msg, counts))
}

fn emit_phase_profile(
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

fn emit_import_cancellation_state(
    app: Option<&AppHandle>,
    job_id: &domain::JobId,
    state: CancellationStateDto,
    safe_to_close: bool,
    detail: &str,
) {
    if let Some(app) = app {
        event_bridge::emit_job_cancellation(
            app,
            &job_cancellation_dto(&job_id.0, state, safe_to_close, detail),
        );
    }
}

fn job_cancellation_dto(
    job_id: &str,
    state: CancellationStateDto,
    safe_to_close: bool,
    detail: &str,
) -> JobCancellationDto {
    let now = chrono::Utc::now().to_rfc3339();
    JobCancellationDto {
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

fn mark_import_cancelling(job_repo: &JobRepo<'_>, job_id: &domain::JobId, detail: &str) {
    if let Err(error) = job_repo.mark_cancelling(job_id, detail) {
        tracing::warn!("Failed to mark job {} as cancelling: {}", job_id.0, error);
    }
}

fn is_import_cancelled_message(message: &str) -> bool {
    message.to_ascii_lowercase().contains("cancel")
}

fn emit_import_profile_progress(
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
        event_bridge::emit_import_phase_progress(app, &phase_progress);
        for result in &phase_progress.partial_results {
            event_bridge::emit_import_partial_result(app, result);
        }
        for status in cache_statuses_from_profile(data_source_id, detail) {
            event_bridge::emit_cache_index_status(app, &status);
        }
        event_bridge::emit_job_progress(app, &job_id.0, progress.min(99), detail);
    }
}

fn import_phase_progress_from_profile(
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
struct PostImportResultCounts {
    timeline_events: u64,
    artifact_count: u64,
    indexed_count: u64,
}

fn partial_results_from_profile(
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

fn cache_statuses_from_profile(
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

fn post_import_counts_from_profile(detail: &str) -> PostImportResultCounts {
    PostImportResultCounts {
        timeline_events: profile_u64(detail, "timeline").unwrap_or(0),
        artifact_count: profile_u64(detail, "artifacts").unwrap_or(0),
        indexed_count: profile_u64(detail, "indexed").unwrap_or(0),
    }
}

fn post_import_counts_from_message(message: &str) -> PostImportResultCounts {
    let normalized = message
        .replace(':', " ")
        .replace('.', " ")
        .replace(',', " ");
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

fn elapsed_ms(duration: Duration) -> u128 {
    duration.as_millis()
}

fn rows_per_sec(rows: u64, duration: Duration) -> u64 {
    let secs = duration.as_secs_f64();
    if secs <= 0.0 {
        rows
    } else {
        (rows as f64 / secs).round() as u64
    }
}

fn bytes_to_mb(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

fn mb_per_sec(bytes: u64, duration: Duration) -> u64 {
    let secs = duration.as_secs_f64();
    if secs <= 0.0 {
        bytes_to_mb(bytes)
    } else {
        ((bytes as f64 / (1024.0 * 1024.0)) / secs).round() as u64
    }
}

fn import_analysis_mode_from_settings(value: &str) -> import_analysis::ImportAnalysisMode {
    match value {
        "budgetedContent" => import_analysis::ImportAnalysisMode::BudgetedContent,
        "fullContent" => import_analysis::ImportAnalysisMode::FullContent,
        _ => import_analysis::ImportAnalysisMode::MetadataOnly,
    }
}

fn prepare_import_config(
    request: &ImportDataSourceRequest,
) -> Result<import_precheck::ImportSourceConfig, CommandError> {
    import_precheck::prepare_import_source_config(request)
        .map_err(import_config_error_to_command_error)
}

fn import_config_error_to_command_error(
    error: import_precheck::ImportSourceConfigError,
) -> CommandError {
    if error.is_invalid_input() {
        CommandError::invalid_input(error.to_string())
    } else {
        CommandError::from_service_error(error)
    }
}

/// Enumerate an image data source (E01/RAW) with partition detection.
#[allow(dead_code)]
fn enumerate_image_data_source<R>(
    conn: &rusqlite::Connection,
    data_source_id: &domain::DataSourceId,
    mut reader: R,
    mut progress: impl FnMut(u32, &str) -> Result<(), String>,
    app: Option<&AppHandle>,
    job_id: Option<&domain::JobId>,
) -> persistence_sqlite::DbResult<file_service::EnumerationStats>
where
    R: EvidenceReader + std::io::Read + std::io::Seek + 'static,
{
    let fs_probe = datasource_service::detect_image_filesystem(&mut reader)
        .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
    let source_path = reader.info().path.clone();
    let source_kind = reader.info().kind.clone();

    if fs_probe.candidates.is_empty() {
        return Ok(file_service::EnumerationStats {
            file_count: 0,
            dir_count: 0,
            total_size: 0,
            warnings: fs_probe.warnings,
        });
    }

    let mut total = file_service::EnumerationStats {
        file_count: 0,
        dir_count: 0,
        total_size: 0,
        warnings: fs_probe.warnings,
    };

    file_service::store_data_source_partitions(conn, data_source_id, &fs_probe.partitions)
        .map_err(persistence_sqlite::DbError::System)?;

    let total_partitions = fs_probe.partitions.len().max(1);
    let mut placeholder_roots = std::collections::HashMap::new();
    for (index, partition) in fs_probe.partitions.iter().enumerate() {
        let root_name = format_partition_record_root_name(partition);
        let detail = match partition.status {
            datasource_service::PartitionStatus::Supported => {
                format!("Detected {root_name}; queued for import")
            }
            datasource_service::PartitionStatus::EncryptedBitLocker => {
                format!("Detected locked {root_name}")
            }
            datasource_service::PartitionStatus::Unsupported => {
                format!("Detected unsupported {root_name}")
            }
        };
        let stage_progress = 12 + (((index as u32) * 8) / total_partitions as u32);
        let progress_detail = if partition.status == datasource_service::PartitionStatus::Supported
        {
            format_partition_progress_detail(
                index as u32,
                total_partitions as u32,
                0,
                &root_name,
                &detail,
            )
        } else {
            detail
        };
        progress(stage_progress, &progress_detail).map_err(persistence_sqlite::DbError::System)?;
        if let (Some(app), Some(jid)) = (app, job_id) {
            let job_repo = persistence_sqlite::repositories::job_repo::JobRepo::new(conn);
            if let Err(e) = job_repo.update_partition_progress(
                jid,
                &root_name,
                index as u32,
                total_partitions as u32,
                0,
            ) {
                tracing::debug!("Failed to update partition progress: {}", e);
            }
            event_bridge::emit_partition_progress(
                app,
                &jid.0,
                &root_name,
                index as u32,
                total_partitions as u32,
                0,
            );
        }
        let status = match partition.status {
            datasource_service::PartitionStatus::Supported => "queued",
            datasource_service::PartitionStatus::EncryptedBitLocker => "locked",
            datasource_service::PartitionStatus::Unsupported => "unsupported",
        };
        let placeholder_id = file_service::insert_partition_placeholder_root(
            conn,
            data_source_id,
            &root_name,
            status,
        )?;
        placeholder_roots.insert(partition.index, placeholder_id);
    }

    let total_candidates = fs_probe.candidates.len().max(1);
    for (index, candidate) in fs_probe.candidates.into_iter().enumerate() {
        let root_name = format_partition_root_name(&candidate);
        let stage_progress = 25 + (((index as u32) * 35) / total_candidates as u32);
        let stage_detail = match candidate.kind {
            ImageFilesystemKind::Ntfs => format!("Enumerating {root_name}"),
            ImageFilesystemKind::Fat => format!("Enumerating {root_name}"),
            ImageFilesystemKind::BitLocker => format!("Skipping locked {root_name}"),
        };
        let progress_detail = format_partition_progress_detail(
            index as u32,
            total_candidates as u32,
            5,
            &root_name,
            &stage_detail,
        );
        progress(stage_progress, &progress_detail).map_err(persistence_sqlite::DbError::System)?;
        if let (Some(app), Some(jid)) = (app, job_id) {
            let job_repo = persistence_sqlite::repositories::job_repo::JobRepo::new(conn);
            if let Err(e) = job_repo.update_partition_progress(
                jid,
                &root_name,
                index as u32,
                total_candidates as u32,
                0,
            ) {
                tracing::debug!("Failed to update partition progress: {}", e);
            }
            event_bridge::emit_partition_progress(
                app,
                &jid.0,
                &root_name,
                index as u32,
                total_candidates as u32,
                0,
            );
        }
        let partition_reader: Box<dyn EvidenceReader> = match source_kind.as_str() {
            "e01" => Box::new(
                E01Reader::open(&source_path)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?,
            ),
            _ => Box::new(
                RawImageReader::open(&source_path)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?,
            ),
        };

        // Create progress callback for partition-level progress updates
        let emit_progress = |pct: u32| {
            if let (Some(a), Some(j)) = (app, job_id) {
                let job_repo = persistence_sqlite::repositories::job_repo::JobRepo::new(conn);
                let overall =
                    25 + ((index as u32 * 35) + (pct * 35 / 100)) / total_candidates.max(1) as u32;
                let _ =
                    job_repo.update_progress(j, overall.min(65), &format!("{root_name} {pct}%"));
                event_bridge::emit_partition_progress(
                    a,
                    &j.0,
                    &root_name,
                    index as u32,
                    total_candidates as u32,
                    pct,
                );
            }
        };
        let progress_cb: Option<&dyn Fn(u32)> = if app.is_some() && job_id.is_some() {
            Some(&emit_progress)
        } else {
            None
        };

        let stats = match candidate.kind {
            ImageFilesystemKind::Ntfs => {
                let fs = fs_ntfs::NtfsReader::open(partition_reader, candidate.offset)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
                enumerate_partition_with_fs(
                    conn,
                    data_source_id,
                    &fs,
                    &root_name,
                    &placeholder_roots,
                    &candidate,
                    progress_cb,
                )?
            }
            ImageFilesystemKind::Fat => {
                let fs = fs_fat::FatReader::open(partition_reader, candidate.offset)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
                enumerate_partition_with_fs(
                    conn,
                    data_source_id,
                    &fs,
                    &root_name,
                    &placeholder_roots,
                    &candidate,
                    progress_cb,
                )?
            }
            ImageFilesystemKind::BitLocker => {
                if let (Some(app), Some(jid)) = (app, job_id) {
                    let job_repo = persistence_sqlite::repositories::job_repo::JobRepo::new(conn);
                    if let Err(e) = job_repo.update_partition_progress(
                        jid,
                        &root_name,
                        (index as u32) + 1,
                        total_candidates as u32,
                        100,
                    ) {
                        tracing::debug!("Failed to update BitLocker partition progress: {}", e);
                    }
                    event_bridge::emit_partition_progress(
                        app,
                        &jid.0,
                        &root_name,
                        (index as u32) + 1,
                        total_candidates as u32,
                        100,
                    );
                }
                continue;
            }
        };
        total.file_count += stats.file_count;
        total.dir_count += stats.dir_count;
        total.total_size += stats.total_size;
        total.warnings.extend(stats.warnings);
        if let (Some(app), Some(jid)) = (app, job_id) {
            let job_repo = persistence_sqlite::repositories::job_repo::JobRepo::new(conn);
            if let Err(e) = job_repo.update_partition_progress(
                jid,
                &root_name,
                (index as u32) + 1,
                total_candidates as u32,
                100,
            ) {
                tracing::debug!("Failed to update partition progress: {}", e);
            }
            event_bridge::emit_partition_progress(
                app,
                &jid.0,
                &root_name,
                (index as u32) + 1,
                total_candidates as u32,
                100,
            );
        }
        let completed_detail = format_partition_progress_detail(
            index as u32,
            total_candidates as u32,
            100,
            &root_name,
            &format!("Imported {root_name}"),
        );
        let completed_progress = stage_progress
            .saturating_add((35 / total_candidates as u32).max(1))
            .min(68);
        progress(completed_progress, &completed_detail)
            .map_err(persistence_sqlite::DbError::System)?;
    }

    if !total.warnings.is_empty() {
        progress(
            60,
            &format!("Partition warnings: {}", total.warnings.join(" | ")),
        )
        .map_err(persistence_sqlite::DbError::System)?;
    }

    Ok(total)
}

/// Build a PartitionWork item for a single partition using pre-computed probe results.
/// Does NOT re-probe the image — uses candidates from the initial detect_image_filesystem call.
fn build_partition_work(
    source_path: &std::path::Path,
    source_kind: &DataSourceKind,
    partition_index: usize,
    partition_name: &str,
    fs_kind: &str,
    probe_candidates: &[datasource_service::ImageFilesystemCandidate],
) -> Option<app_services::parallel_enum::PartitionWork> {
    let candidate = probe_candidates
        .iter()
        .find(|c| c.partition_index.unwrap_or(0) == partition_index)?;

    let fs: Box<dyn FileSystemReader + Send> = match candidate.kind {
        ImageFilesystemKind::Ntfs => {
            let r: Box<dyn EvidenceReader> = if *source_kind == DataSourceKind::E01 {
                Box::new(E01Reader::open(source_path).ok()?)
            } else {
                Box::new(RawImageReader::open(source_path).ok()?)
            };
            Box::new(fs_ntfs::NtfsReader::open(r, candidate.offset).ok()?)
        }
        ImageFilesystemKind::Fat => {
            let r: Box<dyn EvidenceReader> = if *source_kind == DataSourceKind::E01 {
                Box::new(E01Reader::open(source_path).ok()?)
            } else {
                Box::new(RawImageReader::open(source_path).ok()?)
            };
            Box::new(fs_fat::FatReader::open(r, candidate.offset).ok()?)
        }
        _ => return None,
    };

    Some(app_services::parallel_enum::PartitionWork {
        index: partition_index,
        name: partition_name.to_string(),
        fs_kind: fs_kind.to_string(),
        fs,
        source_path: source_path.to_path_buf(),
        source_kind: format!("{:?}", source_kind),
        volume_offset: candidate.offset,
    })
}

fn format_partition_root_name(candidate: &datasource_service::ImageFilesystemCandidate) -> String {
    let partition_label = candidate
        .partition_index
        .map(|index| format!("Partition {}", index))
        .unwrap_or_else(|| "Volume".to_string());
    let fs_label = match candidate.kind {
        ImageFilesystemKind::Ntfs => "NTFS",
        ImageFilesystemKind::Fat => "FAT",
        ImageFilesystemKind::BitLocker => "BitLocker",
    };

    match candidate.partition_name.as_deref() {
        Some(name) if !name.trim().is_empty() => {
            format!("{partition_label} ({fs_label}) - {}", name.trim())
        }
        _ => format!("{partition_label} ({fs_label})"),
    }
}

#[allow(dead_code)]
fn format_partition_record_root_name(partition: &datasource_service::PartitionRecord) -> String {
    let partition_label = format!("Partition {}", partition.index);
    let kind_label = partition.kind_label.trim();

    if partition.name.trim().is_empty() || partition.name.trim() == partition_label {
        format!("{partition_label} ({kind_label})")
    } else {
        format!(
            "{partition_label} ({kind_label}) - {}",
            partition.name.trim()
        )
    }
}

#[allow(dead_code)]
fn format_partition_progress_detail(
    completed_partitions: u32,
    total_partitions: u32,
    partition_progress: u32,
    current_partition: &str,
    detail: &str,
) -> String {
    format!(
        "[partition-progress] {}|{}|{}|{}|{}",
        completed_partitions,
        total_partitions.max(1),
        partition_progress.min(100),
        current_partition,
        detail
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_services::{case_service, search_service};
    use chrono::{DateTime, Utc};
    use persistence_sqlite::repositories::{artifact_repo::ArtifactRepo, job_repo::JobRepo};
    use tempfile::TempDir;

    fn filetime(dt: DateTime<Utc>) -> u64 {
        ((dt.timestamp() + 11_644_473_600) as u64 * 10_000_000)
            + (dt.timestamp_subsec_nanos() as u64 / 100)
    }

    fn assert_partial_result(
        result: &PartialResultDto,
        kind: PartialResultKindDto,
        scope_id: &str,
        ready_count: u64,
        total_estimate: Option<u64>,
        query_key: &str,
        freshness: ResultFreshnessDto,
    ) {
        assert_eq!(result.kind, kind);
        assert_eq!(result.scope_id, scope_id);
        assert_eq!(result.ready_count, ready_count);
        assert_eq!(result.total_estimate, total_estimate);
        assert_eq!(result.query_key, query_key);
        assert_eq!(result.freshness, freshness);
    }

    fn assert_cache_status(
        status: &IndexCacheStatusDto,
        cache_key: &str,
        state: &str,
        indexed_count: u64,
        total_count: Option<u64>,
    ) {
        assert_eq!(status.cache_key, cache_key);
        assert_eq!(status.state, state);
        assert_eq!(status.indexed_count, indexed_count);
        assert_eq!(status.total_count, total_count);
        assert!(chrono::DateTime::parse_from_rfc3339(&status.updated_at).is_ok());
    }

    fn prefetch_fixture(exe_name: &str, run_count: u32, last_run: DateTime<Utc>) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"SCCA");
        data.extend_from_slice(&0x1Eu32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0x0000A000u32.to_le_bytes());

        let mut name_buf = vec![0u8; 60];
        for (index, ch) in exe_name.encode_utf16().enumerate() {
            let offset = index * 2;
            if offset + 1 < name_buf.len() {
                name_buf[offset] = (ch & 0xFF) as u8;
                name_buf[offset + 1] = (ch >> 8) as u8;
            }
        }
        data.extend_from_slice(&name_buf);
        data.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(&run_count.to_le_bytes());
        data.extend_from_slice(&filetime(last_run).to_le_bytes());
        data.extend_from_slice(&[0u8; 7 * 8]);
        data.resize(4096, 0);
        data
    }

    #[test]
    fn import_profile_progress_maps_enumeration_metrics_to_typed_dto() {
        let dto = import_phase_progress_from_profile(
            &domain::JobId("job-1".to_string()),
            &domain::CaseId("case-1".to_string()),
            Some(&domain::DataSourceId("ds-1".to_string())),
            60,
            "Enumeration complete: phase=enumeration elapsedMs=125 rows=12 rowsPerSec=96 dataMb=3 mbPerSec=24 workers=4 rssMb=512",
            false,
        );

        assert_eq!(dto.job_id, "job-1");
        assert_eq!(dto.case_id, "case-1");
        assert_eq!(dto.data_source_id.as_deref(), Some("ds-1"));
        assert_eq!(dto.phase, ImportPhaseDto::Enumerate);
        assert_eq!(dto.state, ImportPhaseStateDto::Completed);
        assert_eq!(dto.percent, 60);
        assert_eq!(dto.metrics.elapsed_ms, 125);
        assert_eq!(dto.metrics.rss_mb, 512);
        assert_eq!(dto.metrics.workers, 4);
        assert_eq!(dto.metrics.rows_processed, 12);
        assert_eq!(dto.metrics.rows_per_sec, Some(96.0));
        assert_eq!(dto.metrics.bytes_processed, 3 * 1024 * 1024);
        assert_eq!(dto.metrics.mb_per_sec, Some(24.0));
        assert!(dto.partial_results.is_empty());
        assert!(dto.cancellable);
        assert!(!dto.cancel_requested);
    }

    #[test]
    fn enum_merge_progress_exposes_ready_file_results() {
        let dto = import_phase_progress_from_profile(
            &domain::JobId("job-files".to_string()),
            &domain::CaseId("case-files".to_string()),
            Some(&domain::DataSourceId("ds-files".to_string())),
            70,
            "File catalog ready: phase=enum-merge rows=9 files=6 dirs=3 warnings=0 rssMb=128",
            false,
        );

        assert_eq!(dto.phase, ImportPhaseDto::MergeEnumeration);
        assert_eq!(dto.state, ImportPhaseStateDto::Completed);
        assert_eq!(dto.partial_results.len(), 2);
        assert_partial_result(
            &dto.partial_results[0],
            PartialResultKindDto::FileRows,
            "ds-files",
            9,
            Some(9),
            "files:rows:ds-files",
            ResultFreshnessDto::Ready,
        );
        assert_partial_result(
            &dto.partial_results[1],
            PartialResultKindDto::FileTree,
            "ds-files",
            9,
            Some(9),
            "files:tree:ds-files",
            ResultFreshnessDto::Ready,
        );
    }

    #[test]
    fn analysis_progress_exposes_partial_search_index_result() {
        let dto = import_phase_progress_from_profile(
            &domain::JobId("job-search".to_string()),
            &domain::CaseId("case-search".to_string()),
            Some(&domain::DataSourceId("ds-search".to_string())),
            75,
            "Analysis heartbeat: phase=analysis scheduling=running memory=ok rssMb=256 workerBudget=4 queuedTasks=2 pendingTasks=5 processed=5/10 indexed=4 activeWorkers=2",
            false,
        );

        assert_eq!(dto.partial_results.len(), 1);
        assert_partial_result(
            &dto.partial_results[0],
            PartialResultKindDto::SearchIndex,
            "ds-search",
            4,
            Some(10),
            "search:index:ds-search",
            ResultFreshnessDto::Partial,
        );
    }

    #[test]
    fn scheduling_profiles_expose_worker_budget_and_deferred_states() {
        let queued = import_phase_progress_from_profile(
            &domain::JobId("job-schedule".to_string()),
            &domain::CaseId("case-schedule".to_string()),
            Some(&domain::DataSourceId("ds-schedule".to_string())),
            72,
            "Analysis staging: phase=analysis-start scheduling=queued mode=budgetedContent workers=3 workerBudget=3 activeWorkers=0 queuedTasks=0 pendingTasks=42 queueBound=768 content=enabled text=enabled contentDeferred=false textDeferred=false rssMb=128",
            false,
        );

        assert_eq!(queued.phase, ImportPhaseDto::Analyze);
        assert_eq!(queued.state, ImportPhaseStateDto::Running);
        assert_eq!(queued.metrics.workers, 3);
        assert_eq!(queued.metrics.rows_total, Some(42));
        assert!(queued.detail.contains("scheduling=queued"));
        assert!(queued.detail.contains("workerBudget=3"));
        assert!(queued.detail.contains("pendingTasks=42"));

        let deferred = import_phase_progress_from_profile(
            &domain::JobId("job-deferred".to_string()),
            &domain::CaseId("case-deferred".to_string()),
            Some(&domain::DataSourceId("ds-deferred".to_string())),
            84,
            "Post-import skipped: phase=post-import-skip scheduling=deferred workerBudget=2 activeWorkers=0 queuedTasks=0 pendingTasks=0 timeline=deferred content=disabled text=disabled contentDeferred=true textDeferred=true",
            false,
        );

        assert_eq!(deferred.phase, ImportPhaseDto::Finalize);
        assert_eq!(deferred.state, ImportPhaseStateDto::Skipped);
        assert_eq!(deferred.metrics.workers, 2);
        assert!(deferred.detail.contains("scheduling=deferred"));
        assert!(deferred.detail.contains("contentDeferred=true"));
        assert!(deferred.detail.contains("textDeferred=true"));
        assert_eq!(deferred.partial_results.len(), 3);
        assert!(deferred
            .partial_results
            .iter()
            .all(|result| result.freshness == ResultFreshnessDto::Deferred));
    }

    #[test]
    fn scheduling_profiles_expose_throttled_and_draining_states() {
        let throttled = import_phase_progress_from_profile(
            &domain::JobId("job-throttle".to_string()),
            &domain::CaseId("case-throttle".to_string()),
            Some(&domain::DataSourceId("ds-throttle".to_string())),
            75,
            "Analysis heartbeat: phase=analysis scheduling=throttled memory=soft-limit rssMb=4096 softLimitMb=4096 hardLimitMb=6144 workerBudget=4 queuedTasks=100 pendingTasks=25 processed=75/100 indexed=20 activeWorkers=4",
            false,
        );

        assert_eq!(throttled.phase, ImportPhaseDto::Analyze);
        assert_eq!(throttled.state, ImportPhaseStateDto::Running);
        assert_eq!(throttled.metrics.workers, 4);
        assert_eq!(throttled.metrics.rows_processed, 75);
        assert_eq!(throttled.metrics.rows_total, Some(100));
        assert!(throttled.detail.contains("scheduling=throttled"));
        assert!(throttled.detail.contains("memory=soft-limit"));

        let draining = import_phase_progress_from_profile(
            &domain::JobId("job-drain".to_string()),
            &domain::CaseId("case-drain".to_string()),
            Some(&domain::DataSourceId("ds-drain".to_string())),
            75,
            "Analysis memory hard limit exceeded: phase=analysis scheduling=draining rssMb=6144 hardLimitMb=6144 workerBudget=4 queuedTasks=100 pendingTasks=25 processed=75 activeWorkers=4",
            true,
        );

        assert_eq!(draining.phase, ImportPhaseDto::Analyze);
        assert_eq!(draining.state, ImportPhaseStateDto::Cancelling);
        assert_eq!(draining.metrics.workers, 4);
        assert!(draining.cancel_requested);
        assert!(draining.detail.contains("scheduling=draining"));
    }

    #[test]
    fn post_import_profiles_expose_deferred_ready_stale_and_invalidated_results() {
        let skipped = partial_results_from_profile(
            Some(&domain::DataSourceId("ds-deferred".to_string())),
            "Post-import skipped: phase=post-import-skip timeline=deferred content=disabled text=disabled",
        );
        assert_eq!(skipped.len(), 3);
        assert_partial_result(
            &skipped[0],
            PartialResultKindDto::TimelineEvents,
            "ds-deferred",
            0,
            None,
            "timeline:events:ds-deferred",
            ResultFreshnessDto::Deferred,
        );
        assert_partial_result(
            &skipped[1],
            PartialResultKindDto::ArtifactFamily,
            "ds-deferred",
            0,
            None,
            "artifacts:family:ds-deferred",
            ResultFreshnessDto::Deferred,
        );
        assert_partial_result(
            &skipped[2],
            PartialResultKindDto::SearchIndex,
            "ds-deferred",
            0,
            None,
            "search:index:ds-deferred",
            ResultFreshnessDto::Deferred,
        );

        let ready = partial_results_from_profile(
            Some(&domain::DataSourceId("ds-ready".to_string())),
            "Post-import complete: phase=post-import elapsedMs=42 timeline=8 artifacts=2 indexed=5 rssMb=128",
        );
        assert_partial_result(
            &ready[0],
            PartialResultKindDto::TimelineEvents,
            "ds-ready",
            8,
            Some(8),
            "timeline:events:ds-ready",
            ResultFreshnessDto::Ready,
        );
        assert_partial_result(
            &ready[1],
            PartialResultKindDto::ArtifactFamily,
            "ds-ready",
            2,
            Some(2),
            "artifacts:family:ds-ready",
            ResultFreshnessDto::Ready,
        );
        assert_partial_result(
            &ready[2],
            PartialResultKindDto::SearchIndex,
            "ds-ready",
            5,
            Some(5),
            "search:index:ds-ready",
            ResultFreshnessDto::Ready,
        );

        let stale = partial_results_from_profile(
            Some(&domain::DataSourceId("ds-stale".to_string())),
            "Analysis staging already merged; skipping analysis resume.",
        );
        assert_partial_result(
            &stale[2],
            PartialResultKindDto::SearchIndex,
            "ds-stale",
            0,
            None,
            "search:index:ds-stale",
            ResultFreshnessDto::Stale,
        );

        let invalidated = partial_results_from_profile(
            Some(&domain::DataSourceId("ds-invalidated".to_string())),
            "Analysis staging layout changed; reinitializing unfinished worker DBs: previousWorkers=[0] currentWorkers=[0, 1]",
        );
        assert_partial_result(
            &invalidated[2],
            PartialResultKindDto::SearchIndex,
            "ds-invalidated",
            0,
            None,
            "search:index:ds-invalidated",
            ResultFreshnessDto::Invalidated,
        );
    }

    #[test]
    fn cache_status_profiles_expose_warming_ready_deferred_reused_stale_and_invalidated_states() {
        let warming = cache_statuses_from_profile(
            Some(&domain::DataSourceId("ds-warming".to_string())),
            "Analysis heartbeat: phase=analysis scheduling=running memory=ok processed=4/10 indexed=3 activeWorkers=2",
        );
        assert_eq!(warming.len(), 3);
        assert_cache_status(
            &warming[2],
            "search:index:ds-warming",
            "warming",
            3,
            Some(10),
        );

        let ready = cache_statuses_from_profile(
            Some(&domain::DataSourceId("ds-ready".to_string())),
            "Post-import complete: phase=post-import elapsedMs=42 timeline=8 artifacts=2 indexed=5 rssMb=128",
        );
        assert_cache_status(&ready[0], "timeline:events:ds-ready", "ready", 8, Some(8));
        assert_cache_status(&ready[1], "artifacts:family:ds-ready", "ready", 2, Some(2));
        assert_cache_status(&ready[2], "search:index:ds-ready", "ready", 5, Some(5));

        let deferred = cache_statuses_from_profile(
            Some(&domain::DataSourceId("ds-deferred".to_string())),
            "Post-import skipped: phase=post-import-skip scheduling=deferred timeline=deferred content=disabled text=disabled",
        );
        assert!(deferred
            .iter()
            .all(|status| status.state == "deferred" && status.total_count.is_none()));

        let reused = cache_statuses_from_profile(
            Some(&domain::DataSourceId("ds-reused".to_string())),
            "Analysis staging already merged; skipping analysis resume.",
        );
        assert!(reused.iter().all(|status| status.state == "reused"));

        let stale = cache_statuses_from_profile(
            Some(&domain::DataSourceId("ds-stale".to_string())),
            "Merging analysis staging DBs...",
        );
        assert!(stale.iter().all(|status| status.state == "stale"));

        let invalidated = cache_statuses_from_profile(
            Some(&domain::DataSourceId("ds-invalidated".to_string())),
            "Analysis staging layout changed; reinitializing unfinished worker DBs: previousWorkers=[0] currentWorkers=[0, 1]",
        );
        assert!(invalidated
            .iter()
            .all(|status| status.state == "invalidated"));
    }

    #[test]
    fn import_profile_progress_maps_analysis_and_cancel_state() {
        let dto = import_phase_progress_from_profile(
            &domain::JobId("job-2".to_string()),
            &domain::CaseId("case-2".to_string()),
            None,
            75,
            "Analysis heartbeat: phase=analysis memory=ok rssMb=256 softLimitMb=1536 hardLimitMb=3072 queuedTasks=2 processed=5/10 indexed=4 activeWorkers=2",
            true,
        );

        assert_eq!(dto.data_source_id, None);
        assert_eq!(dto.phase, ImportPhaseDto::Analyze);
        assert_eq!(dto.state, ImportPhaseStateDto::Cancelling);
        assert_eq!(dto.metrics.rss_mb, 256);
        assert_eq!(dto.metrics.workers, 2);
        assert_eq!(dto.metrics.rows_processed, 5);
        assert_eq!(dto.metrics.rows_total, Some(10));
        assert!(dto.cancel_requested);
    }

    #[test]
    fn import_profile_progress_serializes_as_phase_progress_payload() {
        let dto = import_phase_progress_from_profile(
            &domain::JobId("job-3".to_string()),
            &domain::CaseId("case-3".to_string()),
            Some(&domain::DataSourceId("ds-3".to_string())),
            99,
            "Import profile complete: phase=total elapsedMs=1000 rssMb=128",
            false,
        );
        let value = serde_json::to_value(dto).expect("serialize typed import progress");

        assert_eq!(value["phase"], "finalize");
        assert_eq!(value["state"], "completed");
        assert_eq!(value["percent"], 99);
        assert_eq!(value["cancellable"], false);
        assert_eq!(value["metrics"]["elapsedMs"], 1000);
        assert!(value.get("progress").is_none());
        assert!(value.get("job_id").is_none());
    }

    #[test]
    fn job_cancellation_dto_maps_requested_and_draining_states() {
        let requested = job_cancellation_dto(
            "job-cancel-1",
            CancellationStateDto::Requested,
            false,
            "Cancel requested by user",
        );
        assert_eq!(requested.job_id, "job-cancel-1");
        assert_eq!(requested.state, CancellationStateDto::Requested);
        assert!(!requested.safe_to_close);
        assert!(requested.requested_at.is_some());
        assert!(requested.acknowledged_at.is_none());

        let draining = job_cancellation_dto(
            "job-cancel-1",
            CancellationStateDto::Draining,
            false,
            "Cancellation acknowledged; draining workers",
        );
        assert_eq!(draining.state, CancellationStateDto::Draining);
        assert!(!draining.safe_to_close);
        assert!(draining.requested_at.is_some());
        assert!(draining.acknowledged_at.is_some());
    }

    #[test]
    fn cancellation_after_attach_marks_job_cancelling_without_failure() {
        let tmp = TempDir::new().unwrap();
        let evidence_dir = tmp.path().join("evidence-cancel");
        std::fs::create_dir_all(&evidence_dir).unwrap();
        std::fs::write(evidence_dir.join("notes.txt"), "cancel seam").unwrap();

        let active = case_service::create_case(
            &tmp.path().join("cases"),
            "cancel-after-attach",
            Some("tester"),
        )
        .unwrap();
        let cancel = Arc::new(AtomicBool::new(true));

        active
            .with_conn(|conn| {
                let job_id = JobRepo::new(conn)
                    .create(&active.meta.id.0, "Import cancel")
                    .unwrap();
                let result = execute_import_job(
                    conn,
                    &active.meta.id,
                    &active.case_root,
                    &evidence_dir.to_string_lossy(),
                    &job_id,
                    ImportJobOptions {
                        app: None,
                        cancel_token: &cancel,
                        max_import_workers: None,
                        max_analysis_workers: None,
                        analysis_mode: import_analysis::ImportAnalysisMode::MetadataOnly,
                    },
                );

                assert!(matches!(result, Err(ref error) if error.message.contains("cancelled")));
                let job = JobRepo::new(conn)
                    .list_recent(10)
                    .unwrap()
                    .into_iter()
                    .find(|job| job.id.0 == job_id.0)
                    .unwrap();
                assert_eq!(job.status, "cancelling");
                assert_eq!(job.detail, "Cancellation acknowledged after attach");

                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn logical_import_post_pipeline_indexes_marker_and_extracts_artifact() {
        let tmp = TempDir::new().unwrap();
        let evidence_dir = tmp.path().join("evidence");
        std::fs::create_dir_all(&evidence_dir).unwrap();

        let marker = "fw_marker_8f15d3f2c9e64b51";
        std::fs::write(
            evidence_dir.join("notes.txt"),
            format!("Forensics import marker: {marker}"),
        )
        .unwrap();
        std::fs::write(
            evidence_dir.join("CMD.EXE-DEADBEEF.pf"),
            prefetch_fixture("CMD.EXE", 3, Utc::now()),
        )
        .unwrap();

        let active =
            case_service::create_case(&tmp.path().join("cases"), "post-import", Some("tester"))
                .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));

        active
            .with_conn(|conn| {
                let job_id = JobRepo::new(conn).create(&active.meta.id.0, "Import test")?;
                let message = execute_import_job(
                    conn,
                    &active.meta.id,
                    &active.case_root,
                    &evidence_dir.to_string_lossy(),
                    &job_id,
                    ImportJobOptions {
                        app: None,
                        cancel_token: &cancel,
                        max_import_workers: None,
                        max_analysis_workers: None,
                        analysis_mode: import_analysis::ImportAnalysisMode::MetadataOnly,
                    },
                )
                .map_err(|err| persistence_sqlite::DbError::System(err.message))?;

                assert!(message.contains("Index:"));

                let data_sources: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM data_sources WHERE case_id = ?1 AND kind = 'logical_directory'",
                    [&active.meta.id.0],
                    |row| row.get(0),
                )?;
                assert_eq!(data_sources, 1);

                let file_entries: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM file_entries WHERE entry_type = 'file'",
                    [],
                    |row| row.get(0),
                )?;
                assert_eq!(file_entries, 2);

                let timeline_events: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM timeline_events",
                    [],
                    |row| row.get(0),
                )?;
                assert!(timeline_events > 0);

                let index_dir = active.case_root.join("indexes").join("tantivy");
                let results = search_service::search_files_real(&index_dir, marker, 0, 10)
                    .map_err(persistence_sqlite::DbError::System)?;
                assert_eq!(results.total, 1);
                assert!(results.items[0].path.ends_with("notes.txt"));

                let artifact_repo = ArtifactRepo::new(conn);
                assert!(artifact_repo.count()? > 0);
                let families = artifact_repo.families()?;
                assert!(families.iter().any(|family| family == "Prefetch"));

                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn image_backed_metadata_only_post_import_defers_timeline_until_query() {
        let tmp = TempDir::new().unwrap();
        let active = case_service::create_case(
            &tmp.path().join("cases"),
            "raw-lazy-timeline",
            Some("tester"),
        )
        .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));

        active
            .with_conn(|conn| {
                let _job_id = JobRepo::new(conn).create(&active.meta.id.0, "Raw import seam")?;
                let data_source_id = domain::DataSourceId("raw-ds-1".to_string());
                conn.execute(
                    "INSERT INTO data_sources (id, case_id, name, kind, source_path)
                     VALUES (?1, ?2, 'sample.raw', 'raw', 'C:/evidence/sample.raw')",
                    rusqlite::params![data_source_id.0, active.meta.id.0],
                )?;
                conn.execute(
                    "INSERT INTO file_entries
                     (id, data_source_id, path, name, entry_type, size, ext, deleted,
                      created_at, modified_at, accessed_at, changed_at)
                     VALUES
                     ('raw-file-1', ?1, '/Windows/System32/config/SYSTEM', 'SYSTEM', 'file', 4096,
                      NULL, 0, '2026-01-01T00:00:00Z', '2026-01-02T00:00:00Z',
                      '2026-01-03T00:00:00Z', '2026-01-04T00:00:00Z')",
                    [&data_source_id.0],
                )?;

                let index_dir = active.case_root.join("indexes").join("tantivy");
                let (message, counts) = import_analysis::run_post_import_pipeline_with_counts(
                    import_analysis::PostImportPipelineOptions {
                        case_root: active.case_root.clone(),
                        db_path: active.case_root.join("app.db"),
                        case_id: active.meta.id.0.clone(),
                        data_source_id: data_source_id.clone(),
                        index_dir: index_dir.clone(),
                        max_analysis_workers: Some(1),
                        cancel_token: Arc::clone(&cancel),
                        enable_timeline_projection: false,
                        enable_content_extraction: false,
                        enable_text_indexing: false,
                        analysis_mode: import_analysis::ImportAnalysisMode::MetadataOnly,
                    },
                    None,
                )
                .map_err(|error| persistence_sqlite::DbError::System(error.message))?;

                assert_eq!(
                    message,
                    "Timeline: deferred until Timeline page. Artifacts: 0. Index: 0 indexed"
                );
                assert!(!counts.is_partial());
                let before_query: i64 =
                    conn.query_row("SELECT COUNT(*) FROM timeline_events", [], |row| row.get(0))?;
                assert_eq!(before_query, 0);

                let page = app_services::timeline_service::query_timeline(conn, 0, 10)
                    .map_err(persistence_sqlite::DbError::System)?;
                assert_eq!(page.total, 4);
                assert_eq!(page.items.len(), 4);
                assert!(page
                    .items
                    .iter()
                    .any(|event| event.id == "macb:raw-file-1:FILE_CREATED"));

                let second = app_services::timeline_service::ensure_macb_timeline_projected(conn)
                    .map_err(persistence_sqlite::DbError::System)?;
                assert!(second.already_projected);
                assert_eq!(second.inserted_count, 0);

                Ok(())
            })
            .unwrap();
    }

    #[test]
    #[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
    fn e01_full_import() {
        let e01_path = std::env::var_os("FORENSICS_E01_FIXTURE")
            .map(std::path::PathBuf::from)
            .expect("set FORENSICS_E01_FIXTURE to run real E01 import profile test");
        assert!(
            e01_path.exists(),
            "FORENSICS_E01_FIXTURE does not exist: {}",
            e01_path.display()
        );

        let tmp = tempfile::TempDir::new().unwrap();
        let active =
            case_service::create_case(&tmp.path().join("cases"), "regression", Some("tester"))
                .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));

        eprintln!("=== E01 Full Import Regression Test ===");
        eprintln!("Source: {}", e01_path.display());
        eprintln!("Case ID: {}", active.meta.id.0);

        let t_total = std::time::Instant::now();

        active
            .with_conn(|conn| {
                let job_id = JobRepo::new(conn).create(&active.meta.id.0, "Import regression")?;

                eprintln!("\n[1/5] Starting import...");
                let t_import = std::time::Instant::now();
                let result = execute_import_job(
                    conn,
                    &active.meta.id,
                    &active.case_root,
                    e01_path.to_str().unwrap(),
                    &job_id,
                    ImportJobOptions {
                        app: None,
                        cancel_token: &cancel,
                        max_import_workers: None,
                        max_analysis_workers: None,
                        analysis_mode: import_analysis::ImportAnalysisMode::MetadataOnly,
                    },
                );
                match &result {
                    Ok(msg) => eprintln!(
                        "  Import completed in {:.1}s: {}",
                        t_import.elapsed().as_secs_f64(),
                        msg
                    ),
                    Err(e) => {
                        eprintln!("  Import FAILED: {:?}", e);
                        return Err(persistence_sqlite::DbError::System(format!(
                            "Import failed: {:?}",
                            e
                        )));
                    }
                }

                eprintln!("\n[2/5] Verifying file entries...");
                let file_count: i64 =
                    conn.query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))?;
                eprintln!("  File entries: {}", file_count);
                assert!(file_count > 0, "Expected file entries, got 0");
                let root_system32: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM file_entries
                     WHERE parent_id = 'mft:3:5' AND name = 'System32' COLLATE NOCASE",
                    [],
                    |row| row.get(0),
                )?;
                let root_windows: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM file_entries
                     WHERE parent_id = 'mft:3:5'
                       AND entry_type = 'directory' COLLATE NOCASE
                       AND name = 'Windows' COLLATE NOCASE",
                    [],
                    |row| row.get(0),
                )?;
                let system_hives: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM file_entries
                     WHERE LOWER(REPLACE(path, '\\', '/')) IN (
                       'windows/system32/config/system',
                       'windows/system32/config/software',
                       'windows/system32/winevt/logs/system.evtx'
                     )",
                    [],
                    |row| row.get(0),
                )?;
                eprintln!(
                    "  NTFS shape: root Windows={}, root System32={}, key hives/logs={}",
                    root_windows, root_system32, system_hives
                );
                assert_eq!(
                    root_system32, 0,
                    "System32 must not be flattened under NTFS root"
                );
                assert!(
                    root_windows > 0,
                    "Expected Windows directory under NTFS root"
                );
                assert!(
                    system_hives >= 2,
                    "Expected Windows registry/event-log paths after NTFS import"
                );

                eprintln!("\n[3/5] Verifying timeline lazy projection...");
                let tl_count_before: i64 =
                    conn.query_row("SELECT COUNT(*) FROM timeline_events", [], |row| row.get(0))?;
                eprintln!("  Timeline events before page query: {}", tl_count_before);
                assert_eq!(
                    tl_count_before, 0,
                    "metadata-only import should defer MACB timeline projection"
                );
                let timeline_page = app_services::timeline_service::query_timeline(conn, 0, 10)
                    .map_err(persistence_sqlite::DbError::System)?;
                let tl_count = timeline_page.total as i64;
                eprintln!("  Timeline events after lazy query: {}", tl_count);
                assert!(tl_count > 0, "Expected lazy timeline events, got 0");

                eprintln!("\n[4/6] Verifying system information analysis...");
                let system_info =
                    app_services::analysis_service::extract_system_info_for_case(
                        conn,
                        |file_id, max_bytes| {
                            app_services::file_service::read_file_header_by_id(
                                conn, file_id, max_bytes,
                            )
                        },
                    );
                eprintln!(
                    "  System info: status={:?}, computer={:?}, os={:?}, build={:?}, timezone={:?}, bootRecords={}, warnings={}",
                    system_info.status,
                    system_info.computer_name,
                    system_info.os_version,
                    system_info.build_number,
                    system_info.timezone,
                    system_info.boot_history.len(),
                    system_info.warnings.len()
                );
                for warning in &system_info.warnings {
                    eprintln!("  System info warning: {warning}");
                }
                if system_info.status == transport::dto::AnalysisParseStatusDto::NotParsed
                    || system_info.status == transport::dto::AnalysisParseStatusDto::Unavailable
                {
                    eprintln!(
                        "  System info not parsed for this sample; NTFS import is valid but artifact parsers need follow-up."
                    );
                } else if system_info.status == transport::dto::AnalysisParseStatusDto::Partial {
                    eprintln!(
                        "  System info partially parsed; remaining parser warnings are listed above."
                    );
                }

                eprintln!("\n[5/7] Verifying evidence semantic classification...");
                let evidence_summary =
                    app_services::analysis_service::get_evidence_classification_summary(conn)
                        .map_err(persistence_sqlite::DbError::System)?;
                eprintln!(
                    "  Evidence summary: status={:?}, categories={}, candidates={}, artifacts={}, totalSizeMb={}",
                    evidence_summary.status,
                    evidence_summary.totals.category_count,
                    evidence_summary.totals.candidate_file_count,
                    evidence_summary.totals.artifact_count,
                    evidence_summary.totals.total_size / (1024 * 1024)
                );
                for category in &evidence_summary.categories {
                    if category.file_count > 0 || category.artifact_count > 0 {
                        eprintln!(
                            "    {} status={:?} files={} artifacts={} sources={}",
                            category.category,
                            category.status,
                            category.file_count,
                            category.artifact_count,
                            category.sources.len()
                        );
                    }
                }
                let evidence_category = |name: &str| {
                    evidence_summary
                        .categories
                        .iter()
                        .find(|category| category.category == name)
                        .expect("evidence category should exist")
                };
                assert!(
                    matches!(
                        evidence_category("SystemInformation").status,
                        transport::dto::AnalysisParseStatusDto::CandidateFound
                            | transport::dto::AnalysisParseStatusDto::Parsed
                            | transport::dto::AnalysisParseStatusDto::Partial
                    ),
                    "SystemInformation should not be a fake empty category"
                );
                assert!(
                    matches!(
                        evidence_category("EventLogs").status,
                        transport::dto::AnalysisParseStatusDto::CandidateFound
                            | transport::dto::AnalysisParseStatusDto::Parsed
                            | transport::dto::AnalysisParseStatusDto::Partial
                    ),
                    "EventLogs should not be a fake empty category"
                );
                assert!(
                    evidence_summary.totals.candidate_file_count > 0,
                    "Expected semantic evidence candidates after NTFS import"
                );

                eprintln!("\n[6/7] Verifying optional post-import content outputs...");
                let artifact_count: i64 =
                    conn.query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))?;
                eprintln!("  Artifacts: {}", artifact_count);
                let index_rows: i64 = staging::analysis_staging_db_path(
                    &active.case_root,
                    &{
                        let ds_id: String = conn.query_row(
                            "SELECT id FROM data_sources ORDER BY imported_at DESC LIMIT 1",
                            [],
                            |row| row.get(0),
                        )?;
                        ds_id
                    },
                    0,
                )
                .exists() as i64;
                eprintln!("  Analysis staging exists: {}", index_rows > 0);

                eprintln!("\n[7/7] Verifying job status...");
                let job = JobRepo::new(conn)
                    .list_recent(10)
                    .unwrap()
                    .into_iter()
                    .find(|j| j.id.0 == job_id.0)
                    .unwrap();
                eprintln!("  Job status: {}", job.status);
                assert_eq!(job.status, "running");

                let total_time = t_total.elapsed().as_secs_f64();
                eprintln!("\n=== Regression Test PASSED ===");
                eprintln!("Total time: {:.1}s", total_time);
                eprintln!(
                    "Files: {}, Timeline: {}, Artifacts: {}, SystemInfo={:?}",
                    file_count, tl_count, artifact_count, system_info.status
                );

                Ok(())
            })
            .unwrap();
    }
}
