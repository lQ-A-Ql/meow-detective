use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DataSourceId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DataSourceKind {
    E01,
    Raw,
    LogicalDirectory,
}

impl fmt::Display for DataSourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::E01 => write!(f, "e01"),
            Self::Raw => write!(f, "raw"),
            Self::LogicalDirectory => write!(f, "logical_directory"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSource {
    pub id: DataSourceId,
    pub name: String,
    pub kind: DataSourceKind,
    pub source_path: PathBuf,
    pub imported_at: DateTime<Utc>,
}
