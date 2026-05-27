use app_services::{
    datasource_service::{self, ImageFilesystemKind},
    file_service, search_service, timeline_service,
};
use domain::DataSourceKind;
use evidence_core::{EvidenceReader, LogicalFsReader, RawImageReader};
use image_e01::E01Reader;
use persistence_sqlite::repositories::job_repo::JobRepo;
use std::path::PathBuf;
use tauri::State;
use transport::{
    commands::{GetFileChildrenRequest, GetFileRowsRequest},
    dto::{FileEntryRowDto, FileTreeNodeDto, ViewerHandleDto, ViewerRangeResponseDto},
};

use crate::state::AppState;

fn run_post_import_pipeline(
    conn: &rusqlite::Connection,
    ds_id: &domain::DataSourceId,
    index_dir: &std::path::Path,
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

    // Text indexing (first 1000 files only, MVP)
    let to_index: Vec<domain::FileEntryId> =
        all_files.iter().take(1000).map(|f| f.id.clone()).collect();
    let index_result = search_service::index_files(
        conn,
        index_dir,
        &to_index,
        |_file_id| -> Option<Box<dyn std::io::Read>> { None },
    );

    let index_msg = match index_result {
        Ok(stats) => format!("{} indexed", stats.indexed_count),
        Err(e) => format!("index error: {}", e),
    };

    Ok(format!(
        "Timeline: {} events. Index: {}",
        tl_count, index_msg
    ))
}

#[tauri::command]
pub fn import_data_source(state: State<AppState>, source_path: String) -> Result<String, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    let active = guard.as_ref().ok_or("No active case")?;
    schedule_import_for_active_case(active, &source_path)
}

fn schedule_import_for_active_case(
    active: &app_services::active_case::ActiveCase,
    source_path: &str,
) -> Result<String, String> {
    let case_id = active.meta.id.clone();
    let case_root = active.case_root.clone();
    let db_path = active.db_path();
    let source_name = PathBuf::from(source_path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data_source".to_string());

    let conn = persistence_sqlite::open_or_create(&db_path).map_err(|e| e.to_string())?;
    let job_repo = JobRepo::new(&conn);
    let job_id = job_repo
        .create(&case_id.0, "Import data source")
        .map_err(|e| e.to_string())?;
    job_repo
        .update_progress(&job_id, 1, &format!("Queued import for {source_name}"))
        .map_err(|e| e.to_string())?;

    let source_path = source_path.to_string();
    std::thread::spawn(move || {
        if let Err(error) =
            run_background_import_job(db_path, case_id, case_root, source_path, job_id)
        {
            eprintln!("background import failed: {error}");
        }
    });

    Ok(format!(
        "Import started for {source_name}. Watch the Jobs panel for progress."
    ))
}

#[cfg(test)]
fn import_data_source_for_active_case(
    active: &app_services::active_case::ActiveCase,
    source_path: &str,
) -> Result<String, String> {
    let case_id = active.meta.id.clone();
    let case_root = active.case_root.clone();

    active
        .with_conn(|conn| {
            let job_repo = JobRepo::new(conn);
            let job_id = job_repo.create(&case_id.0, "Import data source")?;
            match execute_import_job(conn, &case_id, &case_root, source_path, &job_id) {
                Ok(message) => {
                    job_repo.complete(&job_id, &message)?;
                    Ok(message)
                }
                Err(error) => {
                    let _ = job_repo.fail(&job_id, &error);
                    Err(persistence_sqlite::DbError::System(error))
                }
            }
        })
        .map_err(|e| e.to_string())
}

fn run_background_import_job(
    db_path: PathBuf,
    case_id: domain::CaseId,
    case_root: PathBuf,
    source_path: String,
    job_id: domain::JobId,
) -> Result<(), String> {
    let conn = persistence_sqlite::open_or_create(&db_path).map_err(|e| e.to_string())?;
    let job_repo = JobRepo::new(&conn);

    match execute_import_job(&conn, &case_id, &case_root, &source_path, &job_id) {
        Ok(message) => job_repo
            .complete(&job_id, &message)
            .map_err(|e| e.to_string()),
        Err(error) => {
            let _ = job_repo.fail(&job_id, &error);
            Err(error)
        }
    }
}

fn execute_import_job(
    conn: &rusqlite::Connection,
    case_id: &domain::CaseId,
    case_root: &std::path::Path,
    source_path: &str,
    job_id: &domain::JobId,
) -> Result<String, String> {
    let path = PathBuf::from(source_path);
    let kind = datasource_service::classify_data_source_path(&path).map_err(|e| e.to_string())?;
    let source_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data_source".to_string());
    let index_dir = case_root.join("indexes").join("tantivy");
    let job_repo = JobRepo::new(conn);

    job_repo
        .update_progress(job_id, 10, &format!("Attaching data source {source_name}"))
        .map_err(|e| e.to_string())?;
    let ds =
        datasource_service::attach_data_source(conn, case_id, &source_name, &path, kind.clone())
            .map_err(|e| e.to_string())?;

    job_repo
        .update_progress(job_id, 25, "Enumerating filesystem...")
        .map_err(|e| e.to_string())?;
    let stats = match kind {
        DataSourceKind::LogicalDirectory => {
            let fs = LogicalFsReader::open(&path, &ds.name).map_err(|e| e.to_string())?;
            file_service::enumerate_filesystem(conn, &ds.id, &fs).map_err(|e| e.to_string())?
        }
        DataSourceKind::E01 => {
            let reader = E01Reader::open(&path).map_err(|e| e.to_string())?;
            enumerate_image_data_source(conn, &ds.id, reader, |progress, detail| {
                job_repo
                    .update_progress(job_id, progress, detail)
                    .map_err(|e| e.to_string())
            })
            .map_err(|e| e.to_string())?
        }
        DataSourceKind::Raw => {
            let reader = RawImageReader::open(&path).map_err(|e| e.to_string())?;
            enumerate_image_data_source(conn, &ds.id, reader, |progress, detail| {
                job_repo
                    .update_progress(job_id, progress, detail)
                    .map_err(|e| e.to_string())
            })
            .map_err(|e| e.to_string())?
        }
    };

    job_repo
        .update_progress(job_id, 70, "Projecting timeline and indexing...")
        .map_err(|e| e.to_string())?;
    let mut msg = format!(
        "Imported: {} files, {} dirs, {} bytes. ",
        stats.file_count, stats.dir_count, stats.total_size
    );
    if !stats.warnings.is_empty() {
        msg.push_str(&format!("Warnings: {}. ", stats.warnings.join("; ")));
    }
    let pipeline_msg =
        run_post_import_pipeline(conn, &ds.id, &index_dir).map_err(|e| e.to_string())?;
    msg.push_str(&pipeline_msg);

    Ok(msg)
}

#[tauri::command]
pub fn get_file_children(
    state: State<AppState>,
    parent_id: String,
) -> Result<Vec<FileTreeNodeDto>, String> {
    get_file_children_request(state, GetFileChildrenRequest { parent_id })
}

#[tauri::command]
pub fn get_file_children_request(
    state: State<AppState>,
    request: GetFileChildrenRequest,
) -> Result<Vec<FileTreeNodeDto>, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    if let Some(active) = guard.as_ref() {
        return active
            .with_conn(|conn| {
                file_service::get_file_children_real(conn, &request.parent_id)
                    .map_err(persistence_sqlite::DbError::System)
            })
            .map_err(|e| e.to_string());
    }
    Ok(vec![])
}

#[tauri::command]
pub fn get_file_tree(state: State<AppState>) -> Result<Vec<FileTreeNodeDto>, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    if let Some(active) = guard.as_ref() {
        let items = active
            .with_conn(|conn| {
                file_service::get_file_tree_real(conn).map_err(persistence_sqlite::DbError::System)
            })
            .map_err(|e| e.to_string())?;
        if !items.is_empty() {
            return Ok(items);
        }
    }
    // Return empty when no data sources exist yet
    Ok(vec![])
}

#[tauri::command]
pub fn get_file_rows(state: State<AppState>) -> Result<Vec<FileEntryRowDto>, String> {
    get_file_rows_request(state, GetFileRowsRequest::default())
}

#[tauri::command]
pub fn get_file_rows_request(
    state: State<AppState>,
    request: GetFileRowsRequest,
) -> Result<Vec<FileEntryRowDto>, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    if let Some(active) = guard.as_ref() {
        return active
            .with_conn(|conn| {
                file_service::get_file_rows_for_request(conn, &request)
                    .map_err(persistence_sqlite::DbError::System)
            })
            .map_err(|e| e.to_string());
    }
    Ok(vec![])
}

#[tauri::command]
pub fn open_file_handle(
    state: State<AppState>,
    file_id: String,
) -> Result<ViewerHandleDto, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    let active = guard.as_ref().ok_or("No active case")?;
    active
        .with_conn(|conn| {
            app_services::file_service::open_file_handle_real(conn, &file_id)
                .map_err(persistence_sqlite::DbError::System)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_file_handle_request(
    state: State<AppState>,
    request: transport::commands::OpenFileHandleRequest,
) -> Result<ViewerHandleDto, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    let active = guard.as_ref().ok_or("No active case")?;
    active
        .with_conn(|conn| {
            app_services::file_service::open_file_handle_real(conn, &request.file_id)
                .map_err(persistence_sqlite::DbError::System)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn read_file_range(
    state: State<AppState>,
    request: transport::dto::ViewerRangeRequestDto,
) -> Result<ViewerRangeResponseDto, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    let Some(active) = guard.as_ref() else {
        return Ok(file_service::read_file_range_real(&request));
    };
    active
        .with_conn(|conn| {
            file_service::read_file_range_for_case(conn, &request)
                .map_err(persistence_sqlite::DbError::System)
        })
        .map_err(|e| e.to_string())
}

fn enumerate_image_data_source<R>(
    conn: &rusqlite::Connection,
    data_source_id: &domain::DataSourceId,
    mut reader: R,
    mut progress: impl FnMut(u32, &str) -> Result<(), String>,
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
        let stats = match candidate.kind {
            ImageFilesystemKind::Ntfs => {
                let fs = fs_ntfs::NtfsReader::open(partition_reader, candidate.offset)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
                if let Some(partition_index) = candidate.partition_index {
                    if let Some(placeholder_id) = placeholder_roots.get(&partition_index) {
                        file_service::replace_placeholder_root_with_real(
                            conn,
                            placeholder_id,
                            &fs,
                            Some(&root_name),
                        )?
                    } else {
                        file_service::enumerate_filesystem_with_root_name(
                            conn,
                            data_source_id,
                            &fs,
                            Some(&root_name),
                        )?
                    }
                } else {
                    file_service::enumerate_filesystem_with_root_name(
                        conn,
                        data_source_id,
                        &fs,
                        Some(&root_name),
                    )?
                }
            }
            ImageFilesystemKind::Fat => {
                let fs = fs_fat::FatReader::open(partition_reader, candidate.offset)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
                if let Some(partition_index) = candidate.partition_index {
                    if let Some(placeholder_id) = placeholder_roots.get(&partition_index) {
                        file_service::replace_placeholder_root_with_real(
                            conn,
                            placeholder_id,
                            &fs,
                            Some(&root_name),
                        )?
                    } else {
                        file_service::enumerate_filesystem_with_root_name(
                            conn,
                            data_source_id,
                            &fs,
                            Some(&root_name),
                        )?
                    }
                } else {
                    file_service::enumerate_filesystem_with_root_name(
                        conn,
                        data_source_id,
                        &fs,
                        Some(&root_name),
                    )?
                }
            }
            ImageFilesystemKind::BitLocker => continue,
        };
        total.file_count += stats.file_count;
        total.dir_count += stats.dir_count;
        total.total_size += stats.total_size;
        total.warnings.extend(stats.warnings);
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
    use super::{import_data_source_for_active_case, schedule_import_for_active_case};
    use app_services::{case_service, file_service};
    use domain::{DataSource, DataSourceId, DataSourceKind};
    use evidence_core::LogicalFsReader;
    use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
    use persistence_sqlite::DbError;
    use std::collections::VecDeque;
    use tempfile::TempDir;
    use transport::{commands::GetFileRowsRequest, dto::ViewerRangeRequestDto};

    fn sample_path() -> std::path::PathBuf {
        "E:/pangushi/刘洋/liuyang_pc.E01".into()
    }

    fn skip() -> bool {
        if !sample_path().exists() {
            eprintln!("SKIP");
            true
        } else {
            false
        }
    }

    #[test]
    fn data_sources_and_recent_objects_are_available_for_case_home() {
        let temp = TempDir::new().unwrap();
        let evidence_dir = temp.path().join("evidence");
        std::fs::create_dir_all(evidence_dir.join("subdir")).unwrap();
        std::fs::write(evidence_dir.join("root.txt"), b"case-home").unwrap();

        let active = case_service::create_case(temp.path(), "home-data", Some("tester")).unwrap();
        let case_id = active.meta.id.clone();

        active
            .with_conn(|conn| {
                let ds = DataSource {
                    id: DataSourceId("ds-home".to_string()),
                    name: "Initial Source".to_string(),
                    kind: DataSourceKind::LogicalDirectory,
                    source_path: evidence_dir.clone(),
                    imported_at: chrono::Utc::now(),
                };
                DataSourceRepo::new(conn).insert(&case_id, &ds)?;
                let fs = LogicalFsReader::open(&evidence_dir, "fixture")
                    .map_err(|e| DbError::System(e.to_string()))?;
                file_service::enumerate_filesystem(conn, &ds.id, &fs)?;

                let sources =
                    file_service::get_data_sources_real(conn, &case_id).map_err(DbError::System)?;
                assert_eq!(sources.len(), 1);
                assert_eq!(sources[0].name, "Initial Source");
                assert_eq!(sources[0].kind, "logical_directory");
                assert!(sources[0].file_count.unwrap_or_default() >= 2);

                file_service::rename_data_source_real(conn, "ds-home", "Renamed Source")
                    .map_err(DbError::System)?;
                let renamed =
                    file_service::get_data_sources_real(conn, &case_id).map_err(DbError::System)?;
                assert_eq!(renamed[0].name, "Renamed Source");

                let recent =
                    file_service::get_recent_objects_real(conn).map_err(DbError::System)?;
                assert!(!recent.is_empty());
                assert_eq!(recent[0].kind, "file");

                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn imports_real_e01_and_browses_files() {
        if skip() {
            return;
        }

        let temp = TempDir::new().unwrap();
        let active = case_service::create_case(temp.path(), "real-import", Some("tester")).unwrap();

        let import_message =
            import_data_source_for_active_case(&active, &sample_path().to_string_lossy()).unwrap();
        eprintln!("import_message={import_message}");

        let tree = active
            .with_conn(|conn| file_service::get_file_tree_real(conn).map_err(DbError::System))
            .unwrap();
        assert!(
            !tree.is_empty(),
            "expected imported image to produce at least one root directory"
        );

        let mut queue: VecDeque<String> = tree.iter().map(|node| node.id.clone()).collect();
        let mut first_file = None;

        while let Some(parent_id) = queue.pop_front() {
            let rows = active
                .with_conn(|conn| {
                    file_service::get_file_rows_for_request(
                        conn,
                        &GetFileRowsRequest {
                            parent_id: Some(parent_id.clone()),
                        },
                    )
                    .map_err(DbError::System)
                })
                .unwrap();

            for row in rows {
                if row.entry_type == "file" {
                    first_file = Some(row);
                    break;
                }
                if row.entry_type == "directory" {
                    queue.push_back(row.id.clone());
                }
            }

            if first_file.is_some() {
                break;
            }
        }

        let first_file = first_file.expect("expected at least one file in imported image");
        let handle = active
            .with_conn(|conn| {
                file_service::open_file_handle_real(conn, &first_file.id).map_err(DbError::System)
            })
            .unwrap();
        assert!(
            handle.handle_id.starts_with("file:"),
            "expected deterministic file handle"
        );

        let range = active
            .with_conn(|conn| {
                file_service::read_file_range_for_case(
                    conn,
                    &ViewerRangeRequestDto {
                        handle_id: handle.handle_id.clone(),
                        offset: 0,
                        length: 64,
                    },
                )
                .map_err(DbError::System)
            })
            .unwrap();
        assert!(
            !range.lines.is_empty(),
            "expected imported file to produce hex preview lines"
        );
    }

    #[test]
    fn import_command_returns_quickly_after_scheduling_job() {
        let temp = TempDir::new().unwrap();
        let evidence_dir = temp.path().join("evidence");
        std::fs::create_dir_all(&evidence_dir).unwrap();
        std::fs::write(evidence_dir.join("hello.txt"), b"hello").unwrap();

        let active =
            case_service::create_case(temp.path(), "async-import", Some("tester")).unwrap();

        let start = std::time::Instant::now();
        let response =
            schedule_import_for_active_case(&active, &evidence_dir.to_string_lossy()).unwrap();
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "import command should return quickly after scheduling background work"
        );
        assert!(
            response.contains("Import started"),
            "expected async import acknowledgement, got: {response}"
        );

        let observed = std::time::Duration::from_secs(10);
        let mut saw_job = false;
        let mut saw_datasource = false;
        let deadline = std::time::Instant::now() + observed;

        while std::time::Instant::now() < deadline {
            let snapshot = active
                .with_conn(|conn| {
                    let jobs = JobRepo::new(conn).list_recent(5)?;
                    let sources = file_service::get_data_sources_real(conn, &active.meta.id)
                        .map_err(DbError::System)?;
                    Ok((jobs, sources))
                })
                .unwrap();

            saw_job = !snapshot.0.is_empty();
            saw_datasource = !snapshot.1.is_empty();
            if saw_job && saw_datasource {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        assert!(saw_job, "expected scheduled import job to be visible");
        assert!(
            saw_datasource,
            "expected background import to attach a data source"
        );
    }

    #[test]
    fn schedules_real_e01_import_and_exposes_tree_without_blocking() {
        if skip() {
            return;
        }

        let temp = TempDir::new().unwrap();
        let active =
            case_service::create_case(temp.path(), "async-real-import", Some("tester")).unwrap();

        let start = std::time::Instant::now();
        let response =
            schedule_import_for_active_case(&active, &sample_path().to_string_lossy()).unwrap();
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "real E01 import should be scheduled quickly"
        );
        assert!(response.contains("Import started"));

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
        let mut saw_running_or_done_job = false;
        let mut saw_tree = false;

        while std::time::Instant::now() < deadline {
            let snapshot = active
                .with_conn(|conn| {
                    let jobs = JobRepo::new(conn).list_recent(5)?;
                    let tree = file_service::get_file_tree_real(conn).map_err(DbError::System)?;
                    Ok((jobs, tree))
                })
                .unwrap();

            saw_running_or_done_job = snapshot.0.iter().any(|job| {
                job.kind == "Import data source"
                    && matches!(job.status.as_str(), "running" | "completed" | "failed")
            });
            saw_tree = !snapshot.1.is_empty();

            if saw_running_or_done_job && saw_tree {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(250));
        }

        assert!(
            saw_running_or_done_job,
            "expected real E01 import job to be observable"
        );
        assert!(
            saw_tree,
            "expected imported real E01 to expose at least one root node"
        );
    }

    #[test]
    fn real_e01_import_eventually_exposes_all_supported_root_partitions() {
        if skip() {
            return;
        }

        let temp = TempDir::new().unwrap();
        let active =
            case_service::create_case(temp.path(), "async-real-partitions", Some("tester"))
                .unwrap();

        schedule_import_for_active_case(&active, &sample_path().to_string_lossy()).unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
        let mut tree_names: Vec<String> = Vec::new();

        while std::time::Instant::now() < deadline {
            tree_names = active
                .with_conn(|conn| {
                    let tree = file_service::get_file_tree_real(conn).map_err(DbError::System)?;
                    Ok(tree.into_iter().map(|node| node.name).collect::<Vec<_>>())
                })
                .unwrap();

            if tree_names.len() >= 3 {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(250));
        }

        assert!(
            tree_names.iter().any(|name| name.contains("Partition 1")),
            "expected Partition 1 root, got {tree_names:?}"
        );
        assert!(
            tree_names.iter().any(|name| name.contains("Partition 3")),
            "expected Partition 3 root, got {tree_names:?}"
        );
        assert!(
            tree_names.iter().any(|name| name.contains("Partition 4")),
            "expected Partition 4 root, got {tree_names:?}"
        );
    }

    #[test]
    fn real_e01_import_exposes_supported_and_locked_partition_roots_early() {
        if skip() {
            return;
        }

        let temp = TempDir::new().unwrap();
        let active =
            case_service::create_case(temp.path(), "async-real-partition-status", Some("tester"))
                .unwrap();

        schedule_import_for_active_case(&active, &sample_path().to_string_lossy()).unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut tree_names: Vec<String> = Vec::new();

        while std::time::Instant::now() < deadline {
            tree_names = active
                .with_conn(|conn| {
                    let tree = file_service::get_file_tree_real(conn).map_err(DbError::System)?;
                    Ok(tree.into_iter().map(|node| node.name).collect::<Vec<_>>())
                })
                .unwrap();

            let has_partition_1 = tree_names.iter().any(|name| name.contains("Partition 1"));
            let has_partition_2 = tree_names.iter().any(|name| name.contains("Partition 2"));
            let has_partition_3 = tree_names.iter().any(|name| name.contains("Partition 3"));
            let has_partition_4 = tree_names.iter().any(|name| name.contains("Partition 4"));
            let has_partition_5 = tree_names.iter().any(|name| name.contains("Partition 5"));

            if has_partition_1
                && has_partition_2
                && has_partition_3
                && has_partition_4
                && has_partition_5
            {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(250));
        }

        assert!(
            tree_names.iter().any(|name| name.contains("Partition 1")),
            "expected Partition 1 root, got {tree_names:?}"
        );
        assert!(
            tree_names.iter().any(|name| name.contains("Partition 2")),
            "expected Partition 2 root, got {tree_names:?}"
        );
        assert!(
            tree_names.iter().any(|name| name.contains("Partition 3")),
            "expected Partition 3 root, got {tree_names:?}"
        );
        assert!(
            tree_names.iter().any(|name| name.contains("Partition 4")),
            "expected Partition 4 root, got {tree_names:?}"
        );
        assert!(
            tree_names.iter().any(|name| name.contains("Partition 5")),
            "expected Partition 5 root, got {tree_names:?}"
        );
    }

    #[test]
    fn real_e01_import_exposes_partition_dtos_on_data_source_summary() {
        if skip() {
            return;
        }

        let temp = TempDir::new().unwrap();
        let active = case_service::create_case(
            temp.path(),
            "async-real-datasource-partitions",
            Some("tester"),
        )
        .unwrap();

        schedule_import_for_active_case(&active, &sample_path().to_string_lossy()).unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut sources = Vec::new();

        while std::time::Instant::now() < deadline {
            sources = active
                .with_conn(|conn| {
                    file_service::get_data_sources_real(conn, &active.meta.id)
                        .map_err(DbError::System)
                })
                .unwrap();

            if sources
                .first()
                .map(|source| source.partitions.len() >= 5)
                .unwrap_or(false)
            {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(250));
        }

        let source = sources.first().expect("expected imported data source");
        assert!(
            source
                .partitions
                .iter()
                .any(|partition| partition.index == 1 && partition.status == "supported"),
            "expected supported partition 1, got {:?}",
            source
                .partitions
                .iter()
                .map(|partition| (&partition.index, &partition.status))
                .collect::<Vec<_>>()
        );
        assert!(
            source
                .partitions
                .iter()
                .any(|partition| partition.index == 2 && partition.status == "unsupported"),
            "expected unsupported partition 2"
        );
        assert!(
            source
                .partitions
                .iter()
                .any(|partition| partition.index == 3 && partition.status == "supported"),
            "expected supported partition 3"
        );
        assert!(
            source
                .partitions
                .iter()
                .any(|partition| partition.index == 4 && partition.status == "supported"),
            "expected supported partition 4"
        );
        assert!(
            source.partitions.iter().any(|partition| {
                partition.index == 5
                    && partition.status == "locked"
                    && partition
                        .unlock_hint
                        .as_deref()
                        .unwrap_or_default()
                        .contains("解锁")
            }),
            "expected locked BitLocker partition 5 with unlock hint"
        );
    }
}
