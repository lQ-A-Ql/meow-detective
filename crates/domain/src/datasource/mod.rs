use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

mod platform;

pub use platform::{DataSourcePlatform, DataSourcePlatformParseError};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DataSourceId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DataSourceKind {
    E01,
    Raw,
    LogicalDirectory,
    CephRbd,
    /// A reconstructed CephFS namespace backed by a cluster source set.
    CephFs,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DataSourceHashStatus {
    Unknown,
    Pending,
    Hashed,
    Failed,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DataSourceProvenanceStatus {
    Unknown,
    Recorded,
    Partial,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataSourceProvenance {
    pub source_hash_sha256: Option<String>,
    pub hash_status: DataSourceHashStatus,
    pub canonical_source_path: Option<PathBuf>,
    pub evidence_size: Option<u64>,
    pub reader_kind: Option<String>,
    pub provenance_status: DataSourceProvenanceStatus,
    pub warnings: Vec<String>,
}

impl DataSourceProvenance {
    pub fn unknown() -> Self {
        Self {
            source_hash_sha256: None,
            hash_status: DataSourceHashStatus::Unknown,
            canonical_source_path: None,
            evidence_size: None,
            reader_kind: None,
            provenance_status: DataSourceProvenanceStatus::Unknown,
            warnings: Vec::new(),
        }
    }
}

impl Default for DataSourceProvenance {
    fn default() -> Self {
        Self::unknown()
    }
}

impl fmt::Display for DataSourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::E01 => write!(f, "e01"),
            Self::Raw => write!(f, "raw"),
            Self::LogicalDirectory => write!(f, "logical_directory"),
            Self::CephRbd => write!(f, "ceph_rbd"),
            Self::CephFs => write!(f, "ceph_fs"),
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
    pub provenance: DataSourceProvenance,
}
