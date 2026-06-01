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
    file_service, search_service, timeline_service,
};
use domain::DataSourceKind;
use evidence_core::{EvidenceReader, LogicalFsReader, RawImageReader};
use image_e01::E01Reader;
use persistence_sqlite::repositories::job_repo::JobRepo;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, State};
use transport::{commands::ImportDataSourceRequest, CommandError};

use crate::events::event_bridge;
use crate::state::AppState;

#[derive(Debug, Clone, Default)]
pub struct JobOutcomeCounts {
    pub warning_count: u32,
    pub skipped_count: u32,
    pub failed_count: u32,
}

impl JobOutcomeCounts {
    fn add_warnings(&mut self, count: usize) {
        self.warning_count = self.warning_count.saturating_add(count as u32);
    }

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

/// Enumerate a filesystem within a partition, handling placeholder root replacement.
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

fn run_post_import_pipeline_with_counts(
    conn: &rusqlite::Connection,
    case_id: &domain::CaseId,
    ds_id: &domain::DataSourceId,
    index_dir: &std::path::Path,
    app: Option<&AppHandle>,
) -> persistence_sqlite::DbResult<(String, JobOutcomeCounts)> {
    let file_repo = persistence_sqlite::repositories::file_repo::FileRepo::new(conn);
    let mut counts = JobOutcomeCounts::default();

    // Read all files for timeline projection
    let roots = file_repo.find_roots(ds_id)?;
    let mut all_files = Vec::new();
    let mut queue = roots;
    while let Some(f) = queue.pop() {
        if f.entry_type != domain::EntryType::Directory {
            all_files.push(f);
        } else {
            let children = file_repo.find_children(&f.id)?;
            queue.extend(children);
        }
    }

    // Timeline projection
    let tl_count = timeline_service::project_and_store_macb(conn, &all_files)
        .map_err(persistence_sqlite::DbError::System)?;

    // Artifact extraction (limited by config)
    let registry = app_services::artifact_service::create_registry();
    let mut sink = artifacts_core::VecSink::new();
    let mut artifact_count = 0u64;
    for file in all_files
        .iter()
        .take(infrastructure::constants::ARTIFACT_EXTRACTION_LIMIT)
    {
        if let Ok(reader) = file_service::open_file_content_by_id(conn, &file.id) {
            match app_services::artifact_service::run_extractors_on_file(
                &registry, &file.id, &file.path, reader, &mut sink,
            ) {
                Ok(stats) => {
                    counts.warning_count = counts.warning_count.saturating_add(stats.warning_count);
                    counts.skipped_count = counts.skipped_count.saturating_add(stats.skipped_count);
                    counts.failed_count = counts.failed_count.saturating_add(stats.failed_count);
                }
                Err(e) => {
                    counts.add_warnings(1);
                    counts.add_skipped(1);
                    tracing::warn!("artifact extraction error for {}: {}", file.path, e);
                }
            }
        } else {
            counts.add_warnings(1);
            counts.add_skipped(1);
        }
    }
    if !sink.artifacts.is_empty() {
        artifact_count = sink.artifacts.len() as u64;
        app_services::artifact_service::store_artifacts(
            conn,
            &sink.artifacts,
            &case_id.0,
            &ds_id.0,
        )
        .map_err(persistence_sqlite::DbError::System)?;
        if let Some(app) = app {
            for artifact in &sink.artifacts {
                event_bridge::emit_artifact_added(app, &artifact.id.0, &artifact.family);
            }
        }
    }

    // Text indexing (limited by config)
    let to_index: Vec<domain::FileEntryId> = all_files
        .iter()
        .take(infrastructure::constants::TEXT_INDEX_LIMIT)
        .map(|f| f.id.clone())
        .collect();
    let index_result = search_service::index_files(conn, index_dir, &to_index, |file_id| {
        file_service::open_file_content_by_id(conn, file_id).ok()
    });

    let index_msg = match index_result {
        Ok(stats) => {
            counts.warning_count = counts.warning_count.saturating_add(stats.warning_count);
            counts.skipped_count = counts.skipped_count.saturating_add(stats.skipped_count);
            counts.failed_count = counts.failed_count.saturating_add(stats.failed_count);
            format!("{} indexed", stats.indexed_count)
        }
        Err(e) => {
            counts.add_warnings(1);
            counts.add_failed(1);
            format!("index error: {}", e)
        }
    };

    let mut message = format!(
        "Timeline: {} events. Artifacts: {}. Index: {}",
        tl_count, artifact_count, index_msg
    );
    if counts.is_partial() {
        message.push_str(&format!(
            ". Partial: {} warnings, {} skipped, {} failed",
            counts.warning_count, counts.skipped_count, counts.failed_count
        ));
    }

    Ok((message, counts))
}

/// Tauri command: Import a data source into the current case.
#[tauri::command]
pub async fn import_data_source(
    state: State<'_, AppState>,
    app: AppHandle,
    request: ImportDataSourceRequest,
) -> Result<String, CommandError> {
    request.validate().map_err(CommandError::invalid_input)?;
    let app_state = state.inner().clone();
    let source_path = request.source_path.clone();
    let app_clone = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
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
        )?;
        Ok(format!(
            "Import started for {}. Watch the Jobs panel for progress.",
            source_path
        ))
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Tauri command: Cancel an in-progress import job.
#[tauri::command]
pub async fn cancel_import(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<String, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if app_state.task_manager.cancel(&job_id) {
            tracing::info!("Cancel requested for job {}", job_id);
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
) -> Result<String, CommandError> {
    let case_id = active.meta.id.clone();
    let case_root = active.case_root.clone();
    let db_path = active.db_path();
    let source_name = PathBuf::from(source_path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data_source".to_string());
    validate_import_source_for_filesystem(source_path)?;

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
    let handle = std::thread::spawn(move || {
        run_background_import_job(
            db_path,
            case_id,
            case_root,
            source_path,
            job_id,
            app_handle.as_ref(),
            &cancel_token_clone,
        )
        .map_err(|e| e.message)
    });

    // Register with TaskManager using the cancel token
    task_manager.register_with_token(job_id_str.clone(), handle, cancel_token);

    Ok(job_id_str)
}

/// Run the background import job in a separate thread.
fn run_background_import_job(
    db_path: PathBuf,
    case_id: domain::CaseId,
    case_root: PathBuf,
    source_path: String,
    job_id: domain::JobId,
    app: Option<&AppHandle>,
    cancel_token: &AtomicBool,
) -> Result<(), CommandError> {
    let conn =
        persistence_sqlite::open_or_create(&db_path).map_err(CommandError::from_service_error)?;
    let job_repo = JobRepo::new(&conn);

    if let Some(app) = app {
        event_bridge::emit_job_started(app, &job_id.0, "Import started");
        event_bridge::emit_job_progress(app, &job_id.0, 5, "Import started");
    }

    // Check for cancellation before starting
    if cancel_token.load(Ordering::Relaxed) {
        let msg = "Import cancelled by user";
        if let Err(e) = job_repo.fail(&job_id, msg) {
            tracing::error!("Failed to mark job {} as cancelled: {}", job_id.0, e);
        }
        if let Some(app) = app {
            event_bridge::emit_job_failed(app, &job_id.0, msg);
        }
        return Ok(());
    }

    match execute_import_job(
        &conn,
        &case_id,
        &case_root,
        &source_path,
        &job_id,
        app,
        cancel_token,
    ) {
        Ok(message) => {
            job_repo
                .complete(&job_id, &message)
                .map_err(CommandError::from_service_error)?;
            if let Some(app) = app {
                event_bridge::emit_job_completed(app, &job_id.0, &message);
            }
            Ok(())
        }
        Err(error) => {
            if let Err(e) = job_repo.fail(&job_id, &error.message) {
                tracing::error!("Failed to mark job {} as failed: {}", job_id.0, e);
            }
            if let Some(app) = app {
                event_bridge::emit_job_failed(app, &job_id.0, &error.message);
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
    app: Option<&AppHandle>,
    cancel_token: &AtomicBool,
) -> Result<String, CommandError> {
    let (message, _counts) = execute_import_job_with_counts(
        conn,
        case_id,
        case_root,
        source_path,
        job_id,
        app,
        cancel_token,
    )?;
    Ok(message)
}

fn execute_import_job_with_counts(
    conn: &rusqlite::Connection,
    case_id: &domain::CaseId,
    case_root: &std::path::Path,
    source_path: &str,
    job_id: &domain::JobId,
    app: Option<&AppHandle>,
    cancel_token: &AtomicBool,
) -> Result<(String, JobOutcomeCounts), CommandError> {
    let path = PathBuf::from(source_path);
    validate_import_source_for_filesystem(source_path)?;
    let kind = datasource_service::classify_data_source_path(&path)
        .map_err(CommandError::from_service_error)?;
    let source_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data_source".to_string());
    let index_dir = case_root.join("indexes").join("tantivy");
    let job_repo = JobRepo::new(conn);
    let mut counts = JobOutcomeCounts::default();

    job_repo
        .update_progress(job_id, 10, &format!("Attaching data source {source_name}"))
        .map_err(CommandError::from_service_error)?;
    if let Some(app) = app {
        event_bridge::emit_job_progress(app, &job_id.0, 10, &format!("Attaching {source_name}"));
    }
    let ds =
        datasource_service::attach_data_source(conn, case_id, &source_name, &path, kind.clone())
            .map_err(CommandError::from_service_error)?;

    // Check for cancellation
    if cancel_token.load(Ordering::Relaxed) {
        return Err(CommandError::internal("Import cancelled by user"));
    }

    job_repo
        .update_progress(job_id, 25, "Enumerating filesystem...")
        .map_err(CommandError::from_service_error)?;
    if let Some(app) = app {
        event_bridge::emit_job_progress(app, &job_id.0, 25, "Enumerating filesystem...");
    }

    let stats = match kind {
        DataSourceKind::LogicalDirectory => {
            let fs =
                LogicalFsReader::open(&path, &ds.name).map_err(CommandError::from_service_error)?;
            file_service::enumerate_filesystem(conn, &ds.id, &fs)
                .map_err(CommandError::from_service_error)?
        }
        DataSourceKind::E01 => {
            let reader = E01Reader::open(&path).map_err(CommandError::from_service_error)?;
            enumerate_image_data_source(
                conn,
                &ds.id,
                reader,
                |progress, detail| {
                    job_repo
                        .update_progress(job_id, progress, detail)
                        .map_err(|e| e.to_string())
                },
                app,
                Some(job_id),
            )
            .map_err(CommandError::from_service_error)?
        }
        DataSourceKind::Raw => {
            let reader = RawImageReader::open(&path).map_err(CommandError::from_service_error)?;
            enumerate_image_data_source(
                conn,
                &ds.id,
                reader,
                |progress, detail| {
                    job_repo
                        .update_progress(job_id, progress, detail)
                        .map_err(|e| e.to_string())
                },
                app,
                Some(job_id),
            )
            .map_err(CommandError::from_service_error)?
        }
    };
    counts.add_warnings(stats.warnings.len());

    // Check for cancellation
    if cancel_token.load(Ordering::Relaxed) {
        return Err(CommandError::internal("Import cancelled by user"));
    }

    job_repo
        .update_progress(job_id, 70, "Running post-import pipeline...")
        .map_err(CommandError::from_service_error)?;
    if let Some(app) = app {
        event_bridge::emit_job_progress(app, &job_id.0, 70, "Running post-import pipeline...");
    }

    let (pipeline_msg, pipeline_counts) =
        run_post_import_pipeline_with_counts(conn, case_id, &ds.id, &index_dir, app)
            .map_err(CommandError::from_service_error)?;
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
    if let Some(app) = app {
        event_bridge::emit_timeline_updated(app, stats.file_count + stats.dir_count);
        event_bridge::emit_search_index_progress(app, 100, "Post-import indexing completed");
    }

    job_repo
        .update_progress(job_id, 95, "Finalizing...")
        .map_err(CommandError::from_service_error)?;
    if let Some(app) = app {
        event_bridge::emit_job_progress(app, &job_id.0, 95, "Finalizing...");
    }

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

fn validate_import_source_for_filesystem(source_path: &str) -> Result<(), CommandError> {
    let path = PathBuf::from(source_path);
    let metadata = std::fs::metadata(&path).map_err(|_| {
        CommandError::invalid_input("sourcePath must exist and be accessible before import")
    })?;
    if metadata.is_dir() || metadata.is_file() {
        Ok(())
    } else {
        Err(CommandError::invalid_input(
            "sourcePath must point to a directory or regular image file",
        ))
    }
}

/// Enumerate an image data source (E01/RAW) with partition detection.
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
        let cancel = AtomicBool::new(false);

        active
            .with_conn(|conn| {
                let job_id = JobRepo::new(conn).create(&active.meta.id.0, "Import test")?;
                let message = execute_import_job(
                    conn,
                    &active.meta.id,
                    &active.case_root,
                    &evidence_dir.to_string_lossy(),
                    &job_id,
                    None,
                    &cancel,
                )
                .map_err(|err| persistence_sqlite::DbError::System(err.message))?;

                assert!(message.contains("Index:"));

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
}
