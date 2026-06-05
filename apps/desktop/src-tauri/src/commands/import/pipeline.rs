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
            event_bridge::emit_job_cancelled(&app, &job_id, "Cancel requested by user");
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
        if let Err(e) = job_repo.fail(&job.job_id, msg) {
            tracing::error!("Failed to mark job {} as cancelled: {}", job.job_id.0, e);
        }
        if let Some(app) = app {
            event_bridge::emit_job_failed(app, &job.job_id.0, msg);
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
        12,
        format!(
            "Attach complete: phase=attach elapsedMs={} rssMb={}",
            elapsed_ms(attach_started.elapsed()),
            import_analysis::current_rss_mb()
        ),
    );

    // Check for cancellation
    if options.cancel_token.load(Ordering::Relaxed) {
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
                    28,
                    format!(
                        "Probe complete: phase=probe elapsedMs={} partitions={} candidates={} rssMb={}",
                        elapsed_ms(probe_started.elapsed()),
                        probe.partitions.len(),
                        probe.candidates.len(),
                        import_analysis::current_rss_mb()
                    ),
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
                        30,
                        format!(
                            "Probe complete: phase=probe-resume elapsedMs={} candidates={} rssMb={}",
                            elapsed_ms(probe_started.elapsed()),
                            probe.candidates.len(),
                            import_analysis::current_rss_mb()
                        ),
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
                    31,
                    format!(
                        "Reader build complete: phase=reader-build elapsedMs={} pending={} failures={} rssMb={}",
                        elapsed_ms(build_started.elapsed()),
                        pending.len(),
                        build_failures.len(),
                        import_analysis::current_rss_mb()
                    ),
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
                    emit_phase_profile(
                        options.app,
                        job_id,
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
                        70,
                        format!(
                            "Partition merge complete: phase=enum-merge elapsedMs={} rows={} rowsPerSec={} rssMb={}",
                            elapsed_ms(enum_merge_elapsed),
                            merged,
                            rows_per_sec(merged, enum_merge_elapsed),
                            import_analysis::current_rss_mb()
                        ),
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

    // Check for cancellation
    if options.cancel_token.load(Ordering::Relaxed) {
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
        emit_import_profile_progress(options.app, job_id, pct, detail);
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
        CommandError::from_service_error(error.message)
    })?;
    let pipeline_counts = JobOutcomeCounts::from(pipeline_counts);
    emit_phase_profile(
        options.app,
        job_id,
        94,
        format!(
            "Post-import complete: phase=post-import elapsedMs={} rssMb={}",
            elapsed_ms(post_import_started.elapsed()),
            import_analysis::current_rss_mb()
        ),
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
        99,
        format!(
            "Import profile complete: phase=total elapsedMs={} rssMb={}",
            elapsed_ms(import_started.elapsed()),
            import_analysis::current_rss_mb()
        ),
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
    progress: u32,
    detail: String,
) {
    emit_import_profile_progress(app, job_id, progress, &detail);
}

fn emit_import_profile_progress(
    app: Option<&AppHandle>,
    job_id: &domain::JobId,
    progress: u32,
    detail: &str,
) {
    tracing::info!("Import profile for {}: {}", job_id.0, detail);
    #[cfg(test)]
    eprintln!("[import-profile] {}% {}", progress.min(99), detail);
    if let Some(app) = app {
        event_bridge::emit_job_progress(app, &job_id.0, progress.min(99), detail);
    }
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
