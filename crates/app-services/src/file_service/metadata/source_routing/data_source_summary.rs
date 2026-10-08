use std::path::Path;

use domain::CaseId;
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, file_repo::FileRepo, partition_repo::PartitionRepo,
};
use rusqlite::Connection;
use transport::dto::{DataSourcePartitionDto, DataSourceSummaryDto};

use crate::file_service::{
    data_sources::{data_source_hash_status_label, data_source_provenance_status_label},
    FileServiceError,
};

use super::shared::open_source_for_data_source;

/// Build sanitized summaries for every data source in a case.
pub fn get_data_sources_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
) -> Result<Vec<DataSourceSummaryDto>, FileServiceError> {
    let sources = DataSourceRepo::new(case_conn).find_by_case(case_id)?;
    let mut summaries = Vec::with_capacity(sources.len());

    for source in sources {
        let storage = DataSourceRepo::new(case_conn)
            .find_storage(&source.id)?
            .ok_or_else(|| {
                FileServiceError::other(format!(
                    "data source {} is missing storage metadata",
                    source.id.0
                ))
            })?;
        let platform = crate::file_service::data_sources::required_data_source_platform(&storage)?;
        let processing = crate::processing_phase_service::get_data_source_processing_summary(
            case_conn, &source.id,
        )?;
        let source_conn =
            open_source_for_data_source(case_conn, case_root, case_id, &source.id).ok();
        let (file_count, partitions) = if let Some(source_conn) = source_conn.as_ref() {
            let file_count = FileRepo::new(source_conn)
                .count_by_data_source(&source.id)
                .ok();
            let partitions = PartitionRepo::new(source_conn)
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
            (file_count, partitions)
        } else {
            (None, Vec::new())
        };

        summaries.push(DataSourceSummaryDto {
            id: source.id.0.clone(),
            name: source.name,
            kind: source.kind.to_string(),
            source_path: source.source_path.display().to_string(),
            imported_at: source.imported_at.to_rfc3339(),
            file_count,
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
        });
    }

    Ok(summaries)
}
