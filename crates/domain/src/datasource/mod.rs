use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DataSourceId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataSourceKind {
    Raw,
    E01,
    LogicalDirectory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSource {
    pub id: DataSourceId,
    pub name: String,
    pub kind: DataSourceKind,
    pub source_path: PathBuf,
    pub imported_at: DateTime<Utc>,
}
