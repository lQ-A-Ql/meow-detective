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

/// Create a file reader function for the given data source type.
///
/// Returns a closure that can read files by their FileEntryId.
/// For logical directories, validates path safety to prevent traversal attacks.
pub fn create_file_reader_fn<'a>(
    source_path: &'a std::path::Path,
    kind: &'a domain::DataSourceKind,
) -> impl Fn(&domain::FileEntryId) -> Option<Box<dyn std::io::Read>> + 'a {
    move |file_id: &domain::FileEntryId| -> Option<Box<dyn std::io::Read>> {
        match kind {
            domain::DataSourceKind::LogicalDirectory => {
                let safe_path = match file_service::safe_relative_path(&file_id.0) {
                    Ok(p) => p,
                    Err(_) => return None,
                };
                let root = match source_path.canonicalize() {
                    Ok(r) => r,
                    Err(_) => return None,
                };
                let full = root.join(safe_path);
                let canonical = match full.canonicalize() {
                    Ok(c) => c,
                    Err(_) => return None,
                };
                if !canonical.starts_with(&root) {
                    return None;
                }
                std::fs::File::open(canonical)
                    .ok()
                    .map(|f| Box::new(f) as Box<dyn std::io::Read>)
            }
            _ => None,
        }
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

/// Run post-import pipeline: timeline projection, artifact extraction, text indexing.
pub fn run_post_import_pipeline(
    conn: &rusqlite::Connection,
    case_id: &domain::CaseId,
    ds_id: &domain::DataSourceId,
    index_dir: &std::path::Path,
    reader_fn: impl Fn(&domain::FileEntryId) -> Option<Box<dyn std::io::Read>>,
) -> persistence_sqlite::DbResult<String> {
    let file_repo = persistence_sqlite::repositories::file_repo::FileRepo::new(conn);

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
        if let Some(reader) = reader_fn(&file.id) {
            if let Err(e) = app_services::artifact_service::run_extractors_on_file(
                &registry, &file.id, &file.path, reader, &mut sink,
            ) {
                tracing::warn!("artifact extraction error for {}: {}", file.path, e);
            }
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
    }

    // Text indexing (limited by config)
    let to_index: Vec<domain::FileEntryId> = all_files
        .iter()
        .take(infrastructure::constants::TEXT_INDEX_LIMIT)
        .map(|f| f.id.clone())
        .collect();
    let index_result = search_service::index_files(conn, index_dir, &to_index, &reader_fn);

    let index_msg = match index_result {
        Ok(stats) => format!("{} indexed", stats.indexed_count),
        Err(e) => format!("index error: {}", e),
    };

    Ok(format!(
        "Timeline: {} events. Artifacts: {}. Index: {}",
        tl_count, artifact_count, index_msg
    ))
}

/// Tauri command: Import a data source into the current case.
#[tauri::command]
pub fn import_data_source(
    state: State<AppState>,
    app: AppHandle,
    request: ImportDataSourceRequest,
) -> Result<String, CommandError> {
    let guard = state
        .active_case
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?;
    let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
    let cancel = Arc::new(AtomicBool::new(false));
    let job_id_str = schedule_import_for_active_case(
        active,
        &request.source_path,
        Some(&app),
        Some(cancel.clone()),
    )?;
    // Store cancel token keyed by job_id
    let mut tokens = state
        .cancel_tokens
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?;
    tokens.insert(job_id_str, cancel);
    Ok(format!(
        "Import started for {}. Watch the Jobs panel for progress.",
        request.source_path
    ))
}

/// Tauri command: Cancel an in-progress import job.
#[tauri::command]
pub fn cancel_import(state: State<AppState>, job_id: String) -> Result<String, CommandError> {
    let tokens = state
        .cancel_tokens
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?;
    if let Some(token) = tokens.get(&job_id) {
        token.store(true, Ordering::Relaxed);
        tracing::info!("Cancel requested for job {}", job_id);
        Ok("Cancel requested".to_string())
    } else {
        Err(CommandError::not_found("Job"))
    }
}

/// Schedule an import job for the active case.
pub fn schedule_import_for_active_case(
    active: &app_services::active_case::ActiveCase,
    source_path: &str,
    app: Option<&AppHandle>,
    cancel_token: Option<Arc<AtomicBool>>,
) -> Result<String, CommandError> {
    let case_id = active.meta.id.clone();
    let case_root = active.case_root.clone();
    let db_path = active.db_path();
    let source_name = PathBuf::from(source_path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data_source".to_string());

    let conn =
        persistence_sqlite::open_or_create(&db_path).map_err(CommandError::from_service_error)?;
    let job_repo = JobRepo::new(&conn);
    let job_id = job_repo
        .create(&case_id.0, "Import data source")
        .map_err(CommandError::from_service_error)?;
    let job_id_str = job_id.0.clone();
    job_repo
        .update_progress(&job_id, 1, &format!("Queued import for {source_name}"))
        .map_err(CommandError::from_service_error)?;

    let source_path = source_path.to_string();
    let app_handle = app.cloned();
    let cancel = cancel_token.unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
    std::thread::spawn(move || {
        if let Err(error) = run_background_import_job(
            db_path,
            case_id,
            case_root,
            source_path,
            job_id,
            app_handle.as_ref(),
            cancel,
        ) {
            tracing::error!("background import failed: {}", error);
        }
    });

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
    cancel: Arc<AtomicBool>,
) -> Result<(), CommandError> {
    let conn =
        persistence_sqlite::open_or_create(&db_path).map_err(CommandError::from_service_error)?;
    let job_repo = JobRepo::new(&conn);

    if let Some(app) = app {
        event_bridge::emit_job_progress(app, &job_id.0, 5, "Import started");
    }

    if cancel.load(Ordering::Relaxed) {
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
        &cancel,
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
    cancel: &AtomicBool,
) -> Result<String, CommandError> {
    let path = PathBuf::from(source_path);
    let kind = datasource_service::classify_data_source_path(&path)
        .map_err(CommandError::from_service_error)?;
    let source_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data_source".to_string());
    let index_dir = case_root.join("indexes").join("tantivy");
    let job_repo = JobRepo::new(conn);

    job_repo
        .update_progress(job_id, 10, &format!("Attaching data source {source_name}"))
        .map_err(CommandError::from_service_error)?;
    if let Some(app) = app {
        event_bridge::emit_job_progress(app, &job_id.0, 10, &format!("Attaching {source_name}"));
    }
    let ds =
        datasource_service::attach_data_source(conn, case_id, &source_name, &path, kind.clone())
            .map_err(CommandError::from_service_error)?;

    job_repo
        .update_progress(job_id, 25, "Enumerating filesystem...")
        .map_err(CommandError::from_service_error)?;
    if let Some(app) = app {
        event_bridge::emit_job_progress(app, &job_id.0, 25, "Enumerating filesystem...");
    }

    if cancel.load(Ordering::Relaxed) {
        return Err(CommandError::internal("Import cancelled by user"));
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

    job_repo
        .update_progress(job_id, 70, "Running post-import pipeline...")
        .map_err(CommandError::from_service_error)?;
    if let Some(app) = app {
        event_bridge::emit_job_progress(app, &job_id.0, 70, "Running post-import pipeline...");
    }

    let reader_fn = create_file_reader_fn(&path, &kind);
    let pipeline_msg =
        run_post_import_pipeline(conn, case_id, &ds.id, &index_dir, reader_fn)
            .map_err(CommandError::from_service_error)?;

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

    Ok(msg)
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
