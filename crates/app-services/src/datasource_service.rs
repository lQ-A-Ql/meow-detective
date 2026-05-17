use domain::{CaseId, DataSource, DataSourceId, DataSourceKind};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DataSourceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("Evidence error: {0}")]
    Evidence(String),
}

pub type Result<T> = std::result::Result<T, DataSourceError>;

pub fn attach_data_source(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    name: &str,
    source_path: &Path,
    kind: DataSourceKind,
) -> Result<DataSource> {
    let id = DataSourceId(uuid::Uuid::new_v4().to_string());
    let ds = DataSource {
        id: id.clone(),
        name: name.to_string(),
        kind,
        source_path: source_path.to_path_buf(),
        imported_at: chrono::Utc::now(),
    };

    DataSourceRepo::new(conn).insert(case_id, &ds)?;
    Ok(ds)
}
