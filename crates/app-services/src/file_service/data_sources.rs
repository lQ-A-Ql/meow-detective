use crate::file_service::FileServiceError;
use crate::source_db::{self, GlobalFileId};
use domain::{
    DataSourceHashStatus, DataSourceId, DataSourcePlatform, DataSourceProvenanceStatus, EntryType,
    FileEntry,
};
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    file_repo::FileRepo,
    partition_repo::PartitionRepo,
};
use rusqlite::Connection;
use std::path::Path;
use transport::dto::{DataSourcePartitionDto, DataSourceSummaryDto, RecentObjectDto};

pub fn get_data_sources_real(
    conn: &Connection,
    case_id: &domain::CaseId,
) -> Result<Vec<DataSourceSummaryDto>, FileServiceError> {
    let ds_repo = DataSourceRepo::new(conn);
    let file_repo = FileRepo::new(conn);
    let partition_repo = PartitionRepo::new(conn);
    let sources = ds_repo.find_by_case(case_id)?;

    sources
        .into_iter()
        .map(|source| {
            let storage = ds_repo.find_storage(&source.id)?.ok_or_else(|| {
                FileServiceError::other(format!(
                    "data source {} is missing storage metadata",
                    source.id.0
                ))
            })?;
            let platform = required_data_source_platform(&storage)?;
            let processing = crate::processing_phase_service::get_data_source_processing_summary(
                conn, &source.id,
            )?;
            let partitions = partition_repo
                .find_by_data_source(&source.id.0)
                .map(|items| {
                    items
                        .into_iter()
                        .map(|item| DataSourcePartitionDto {
                            index: item.partition_index,
                            name: item.name,
                            kind_label: item.kind_label,
                            status: item.status,
                            offset: item.offset,
                            length: item.length,
                            type_guid: item.type_guid,
                            filesystem: item.filesystem,
                            unlock_hint: item.unlock_hint,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            Ok(DataSourceSummaryDto {
                id: source.id.0.clone(),
                name: source.name,
                kind: source.kind.to_string(),
                source_path: source.source_path.display().to_string(),
                imported_at: source.imported_at.to_rfc3339(),
                file_count: file_repo.count_by_data_source(&source.id).ok(),
                storage_model: Some(storage.storage_model),
                source_db_rel_path: storage.source_db_rel_path,
                index_rel_path: storage.index_rel_path,
                staging_rel_path: storage.staging_rel_path,
                platform,
                profile: storage.profile,
                import_state: Some(storage.import_state),
                schema_version: storage.schema_version,
                last_error: storage.last_error,
                processing,
                source_hash: source.provenance.source_hash_sha256,
                hash_status: Some(data_source_hash_status_label(
                    &source.provenance.hash_status,
                )),
                canonical_path: source
                    .provenance
                    .canonical_source_path
                    .map(|path| path.display().to_string()),
                evidence_size: source.provenance.evidence_size,
                reader_kind: source.provenance.reader_kind,
                provenance_status: Some(data_source_provenance_status_label(
                    &source.provenance.provenance_status,
                )),
                warnings: source.provenance.warnings,
                partitions,
            })
        })
        .collect()
}

pub(crate) fn required_data_source_platform(
    storage: &DataSourceStorage,
) -> Result<String, FileServiceError> {
    DataSourcePlatform::parse_explicit(&storage.platform)
        .map(|platform| platform.as_storage_str().to_string())
        .map_err(|error| FileServiceError::other(error.to_string()))
}

pub(crate) fn data_source_hash_status_label(status: &DataSourceHashStatus) -> String {
    match status {
        DataSourceHashStatus::Unknown => "unknown",
        DataSourceHashStatus::Pending => "pending",
        DataSourceHashStatus::Hashed => "hashed",
        DataSourceHashStatus::Failed => "failed",
        DataSourceHashStatus::Unavailable => "unavailable",
    }
    .to_string()
}

pub(crate) fn data_source_provenance_status_label(status: &DataSourceProvenanceStatus) -> String {
    match status {
        DataSourceProvenanceStatus::Unknown => "unknown",
        DataSourceProvenanceStatus::Recorded => "recorded",
        DataSourceProvenanceStatus::Partial => "partial",
        DataSourceProvenanceStatus::Failed => "failed",
    }
    .to_string()
}

pub fn rename_data_source_real(
    conn: &Connection,
    data_source_id: &str,
    name: &str,
) -> Result<(), FileServiceError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(FileServiceError::invalid_input(
            "Data source name cannot be empty",
        ));
    }

    DataSourceRepo::new(conn).rename(&DataSourceId(data_source_id.to_string()), trimmed)?;
    Ok(())
}

pub fn get_recent_objects_real(
    conn: &Connection,
) -> Result<Vec<RecentObjectDto>, FileServiceError> {
    let file_repo = FileRepo::new(conn);
    let roots = file_repo.find_root_entries()?;
    let mut recent = Vec::new();
    let mut queue: std::collections::VecDeque<FileEntry> = roots.into();

    while let Some(entry) = queue.pop_front() {
        if entry.entry_type == EntryType::Directory {
            let children = file_repo.find_children(&entry.id)?;
            queue.extend(children);
            continue;
        }

        let timestamp = entry
            .modified_at
            .or(entry.created_at)
            .or(entry.accessed_at)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| "-".to_string());

        recent.push(RecentObjectDto {
            id: entry.id.0.clone(),
            title: entry.name.clone(),
            detail: if entry.path.is_empty() {
                entry.name.clone()
            } else {
                entry.path.clone()
            },
            time: timestamp,
            kind: "file".to_string(),
        });

        if recent.len() >= 8 {
            break;
        }
    }

    Ok(recent)
}

pub fn get_recent_objects_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
) -> Result<Vec<RecentObjectDto>, FileServiceError> {
    let mut recent = Vec::new();

    for (source, _) in source_db::ready_data_sources(case_conn, case_id)? {
        let source_conn =
            source_db::open_ready_source_by_id(case_conn, case_root, case_id, &source.id)?;
        let mut source_recent = get_recent_objects_real(&source_conn.connection)?;
        for item in &mut source_recent {
            item.id = GlobalFileId::new(source.id.clone(), domain::FileEntryId(item.id.clone()))
                .encode()
                .0;
            item.detail = format!("{} · {}", source.name, item.detail);
        }
        recent.extend(source_recent);
    }

    recent.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| a.id.cmp(&b.id)));
    recent.truncate(8);
    Ok(recent)
}
