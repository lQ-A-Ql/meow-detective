use domain::{CaseId, DataSource, DataSourceId, DataSourceKind};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DataSourceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
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

    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![ds.id.0, case_id.0, ds.name, format_ds_kind(&ds.kind), ds.source_path.display().to_string()],
    )?;

    Ok(ds)
}

fn format_ds_kind(kind: &DataSourceKind) -> &'static str {
    match kind {
        DataSourceKind::Raw => "raw",
        DataSourceKind::E01 => "e01",
        DataSourceKind::LogicalDirectory => "logical_directory",
    }
}
