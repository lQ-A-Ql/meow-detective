use domain::{CaseId, DataSourceId, NestedEvidenceLineage};
use persistence_fjall::{ParserRecord, ScanDirection};
use persistence_sqlite::repositories::nested_evidence_repo::NestedEvidenceRepo;
use rusqlite::Connection;
use thiserror::Error;
use transport::dto::NestedEvidenceLineageDto;
use transport::{ErrorCategory, ServiceErrorCategory};

const NESTED_LINEAGE_FAMILY: &str = "nested";
const NESTED_LINEAGE_KIND: &str = "lineage";
const MAX_NESTED_LINEAGE_ROWS: usize = 4_096;

mod materialize;
mod platform;
mod reuse;
pub use materialize::{materialize_fixed_vhd, materialize_fixed_vhd_with_cancel, MaterializeError};

#[derive(Debug, Error)]
pub enum NestedEvidenceError {
    #[error("nested evidence task cancelled")]
    Cancelled,
    #[error("database error: {0}")]
    Database(#[from] persistence_sqlite::DbError),
    #[error("parent data source is not part of the active case")]
    WrongCase,
    #[error("nested evidence materialization failed: {0}")]
    Materialization(#[source] MaterializeError),
}

impl ServiceErrorCategory for NestedEvidenceError {
    fn category(&self) -> ErrorCategory {
        match self {
            Self::Database(_) => ErrorCategory::Io,
            Self::WrongCase => ErrorCategory::Security,
            Self::Materialization(error) => match error {
                MaterializeError::Cancelled => ErrorCategory::Cancelled,
                MaterializeError::Io(_)
                | MaterializeError::Database(_)
                | MaterializeError::Sqlite(_) => ErrorCategory::Io,
                MaterializeError::Source(_) | MaterializeError::Conflict => ErrorCategory::Security,
                MaterializeError::NotFile | MaterializeError::TooLarge => ErrorCategory::Validation,
                MaterializeError::UnsupportedFormat => ErrorCategory::Unsupported,
            },
            Self::Cancelled => ErrorCategory::Cancelled,
        }
    }
    fn user_message(&self) -> Option<&'static str> {
        Some("Nested evidence operation failed")
    }
}

pub fn list_for_parent(
    conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    parent_id: &str,
) -> Result<Vec<NestedEvidenceLineageDto>, NestedEvidenceError> {
    let parent = DataSourceId(parent_id.to_string());
    let owner = persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn)
        .case_id(&parent)?;
    if owner != *case_id {
        return Err(NestedEvidenceError::WrongCase);
    }
    if let Ok(source) = crate::source_db::open_registered_fjall_source_read_only(case_root, &parent)
    {
        let snapshot = source.snapshot();
        let page = source
            .parser_list_by_kind(
                &snapshot,
                &parent.0,
                NESTED_LINEAGE_KIND,
                None,
                ScanDirection::Forward,
                MAX_NESTED_LINEAGE_ROWS.saturating_add(1),
            )
            .map_err(|error| {
                NestedEvidenceError::Database(persistence_sqlite::DbError::System(
                    error.to_string(),
                ))
            })?;
        if page.records.len() > MAX_NESTED_LINEAGE_ROWS {
            return Err(NestedEvidenceError::Database(
                persistence_sqlite::DbError::System(
                    "nested evidence lineage exceeds the bounded query limit".to_owned(),
                ),
            ));
        }
        return page.records.into_iter().map(parser_record_to_dto).collect();
    }
    Ok(NestedEvidenceRepo::new(conn)
        .find_by_parent(&parent)?
        .into_iter()
        .map(to_dto)
        .collect())
}

pub(crate) fn lineage_parser_record(lineage: &NestedEvidenceLineage) -> ParserRecord {
    let payload = serde_json::json!({
        "parentDataSourceId": lineage.parent_data_source_id.0,
        "nestedFilePath": lineage.nested_file_path,
        "parentFileId": lineage.parent_file_id,
        "derivedDataSourceId": lineage.derived_data_source_id.as_ref().map(|id| id.0.clone()),
        "offset": lineage.offset,
        "length": lineage.length,
        "probeKind": lineage.probe_kind,
    });
    ParserRecord {
        id: format!("lineage:{}", lineage.nested_file_path),
        data_source_id: lineage.parent_data_source_id.0.clone(),
        family: NESTED_LINEAGE_FAMILY.to_owned(),
        kind: NESTED_LINEAGE_KIND.to_owned(),
        status: "ready".to_owned(),
        summary_sha256: infrastructure::hashing::sha256_bytes(payload.to_string().as_bytes()),
        payload,
    }
}

fn parser_record_to_dto(
    record: ParserRecord,
) -> Result<NestedEvidenceLineageDto, NestedEvidenceError> {
    let payload = record.payload;
    let text = |name: &str| {
        payload
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let parent_data_source_id = text("parentDataSourceId").ok_or_else(|| {
        NestedEvidenceError::Database(persistence_sqlite::DbError::System(
            "nested lineage parser record is missing parent identity".to_owned(),
        ))
    })?;
    let nested_file_path = text("nestedFilePath").ok_or_else(|| {
        NestedEvidenceError::Database(persistence_sqlite::DbError::System(
            "nested lineage parser record is missing path".to_owned(),
        ))
    })?;
    let probe_kind = text("probeKind").ok_or_else(|| {
        NestedEvidenceError::Database(persistence_sqlite::DbError::System(
            "nested lineage parser record is missing probe kind".to_owned(),
        ))
    })?;
    let offset = payload
        .get("offset")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            NestedEvidenceError::Database(persistence_sqlite::DbError::System(
                "nested lineage parser record has invalid offset".to_owned(),
            ))
        })?;
    let length = payload
        .get("length")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            NestedEvidenceError::Database(persistence_sqlite::DbError::System(
                "nested lineage parser record has invalid length".to_owned(),
            ))
        })?;
    Ok(NestedEvidenceLineageDto {
        parent_data_source_id,
        nested_file_path,
        parent_file_id: text("parentFileId"),
        derived_data_source_id: text("derivedDataSourceId"),
        offset,
        length,
        probe_kind,
    })
}

fn to_dto(value: NestedEvidenceLineage) -> NestedEvidenceLineageDto {
    NestedEvidenceLineageDto {
        parent_data_source_id: value.parent_data_source_id.0,
        nested_file_path: value.nested_file_path,
        parent_file_id: value.parent_file_id,
        derived_data_source_id: value.derived_data_source_id.map(|v| v.0),
        offset: value.offset,
        length: value.length,
        probe_kind: value.probe_kind,
    }
}
