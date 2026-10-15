use domain::{CaseId, DataSourceId, NestedEvidenceLineage};
use persistence_sqlite::repositories::nested_evidence_repo::NestedEvidenceRepo;
use rusqlite::Connection;
use thiserror::Error;
use transport::dto::NestedEvidenceLineageDto;
use transport::{ErrorCategory, ServiceErrorCategory};

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
    case_id: &CaseId,
    parent_id: &str,
) -> Result<Vec<NestedEvidenceLineageDto>, NestedEvidenceError> {
    let parent = DataSourceId(parent_id.to_string());
    let owner = persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn)
        .case_id(&parent)?;
    if owner != *case_id {
        return Err(NestedEvidenceError::WrongCase);
    }
    Ok(NestedEvidenceRepo::new(conn)
        .find_by_parent(&parent)?
        .into_iter()
        .map(to_dto)
        .collect())
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
