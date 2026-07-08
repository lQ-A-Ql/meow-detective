use crate::file_service::FileServiceError;
use crate::source_db::{self, GlobalFileId};
use domain::{
    DataSourceHashStatus, DataSourceId, DataSourceProvenanceStatus, EntryType, FileEntry,
};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, file_repo::FileRepo, partition_repo::PartitionRepo,
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

    Ok(sources
        .into_iter()
        .map(|source| {
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

            DataSourceSummaryDto {
                id: source.id.0.clone(),
                name: source.name,
                kind: source.kind.to_string(),
                source_path: source.source_path.display().to_string(),
                imported_at: source.imported_at.to_rfc3339(),
                file_count: file_repo.count_by_data_source(&source.id).ok(),
                storage_model: None,
                source_db_rel_path: None,
                index_rel_path: None,
                staging_rel_path: None,
                platform: None,
                profile: None,
                import_state: None,
                schema_version: None,
                last_error: None,
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
            }
        })
        .collect())
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
    let sources = DataSourceRepo::new(case_conn).find_by_case(case_id)?;
    let mut recent = Vec::new();

    for source in sources {
        let storage = DataSourceRepo::new(case_conn).find_storage(&source.id)?;
        if storage
            .as_ref()
            .is_some_and(|value| value.import_state == "failed")
        {
            continue;
        }
        let source_conn =
            match source_db::open_registered_source_db(case_conn, case_root, &source.id) {
                Ok(conn) => conn,
                Err(_) => continue,
            };
        let mut source_recent = get_recent_objects_real(&source_conn)?;
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
