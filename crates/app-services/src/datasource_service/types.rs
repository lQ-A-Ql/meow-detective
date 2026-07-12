use domain::DataSourceKind;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DataSourceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("Evidence error: {0}")]
    Evidence(String),
    #[error("Unsupported data source platform: {0}")]
    UnsupportedPlatform(String),
    #[error(
        "Ceph BlueStore OSD block device detected; RADOS/PG/object reconstruction is not supported"
    )]
    UnsupportedCephBlueStore,
}

impl transport::ServiceErrorCategory for DataSourceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Io(_) | Self::Db(_) => transport::ErrorCategory::Io,
            Self::Evidence(_) => transport::ErrorCategory::Validation,
            Self::UnsupportedPlatform(_) | Self::UnsupportedCephBlueStore => {
                transport::ErrorCategory::Unsupported
            }
        }
    }

    fn code(&self) -> Option<&'static str> {
        match self {
            Self::UnsupportedCephBlueStore => Some("CEPH_BLUESTORE_UNSUPPORTED"),
            _ => None,
        }
    }

    fn user_message(&self) -> Option<&'static str> {
        match self {
            Self::UnsupportedCephBlueStore => Some(
                "Ceph BlueStore OSD block device detected; RADOS/PG/object reconstruction is not supported",
            ),
            _ => None,
        }
    }

    fn recoverable(&self) -> Option<bool> {
        match self {
            Self::UnsupportedCephBlueStore => Some(false),
            _ => None,
        }
    }

    fn safe_details(&self) -> Option<serde_json::Value> {
        match self {
            Self::UnsupportedCephBlueStore => Some(serde_json::json!({
                "format": "cephBlueStore",
                "deviceRole": "osdBlock",
                "filesystem": false,
                "missingCapability": "radosPgObjectReconstruction"
            })),
            _ => None,
        }
    }

    fn suggestion(&self) -> Option<&'static str> {
        match self {
            Self::UnsupportedCephBlueStore => Some(
                "Import the PVE host-system disks separately; BlueStore OSD object reconstruction requires a dedicated Ceph analysis workflow.",
            ),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, DataSourceError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFilesystemKind {
    Ntfs,
    Fat,
    BitLocker,
    Ext4,
    Xfs,
    Btrfs,
    LvmPool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFilesystemSource {
    DirectVolume,
    MbrPartition,
    GptPartition,
    LvmLogicalVolume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedImageKind {
    CephBlueStore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedImageVolume {
    pub kind: UnsupportedImageKind,
    pub source: ImageFilesystemSource,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LvmLogicalVolumeIdentity {
    pub vg_uuid: String,
    pub vg_name: String,
    pub lv_uuid: String,
    pub lv_name: String,
    pub pv_offsets: Vec<u64>,
    pub pv_sources: Vec<LvmPhysicalVolumeSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LvmPhysicalVolumeSource {
    pub source_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<DataSourceKind>,
    pub offset: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pv_uuid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pv_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFilesystemCandidate {
    pub partition_index: Option<usize>,
    pub partition_name: Option<String>,
    pub kind: ImageFilesystemKind,
    pub offset: u64,
    pub source: ImageFilesystemSource,
    pub lvm_identity: Option<LvmLogicalVolumeIdentity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionStatus {
    Supported,
    Expanded,
    EncryptedBitLocker,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionRecord {
    pub index: usize,
    pub name: String,
    pub kind_label: String,
    pub type_guid: Option<String>,
    pub offset: u64,
    pub length: u64,
    pub status: PartitionStatus,
    pub filesystem: Option<ImageFilesystemKind>,
    pub lvm_identity: Option<LvmLogicalVolumeIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFilesystemProbe {
    pub candidates: Vec<ImageFilesystemCandidate>,
    pub partitions: Vec<PartitionRecord>,
    pub unsupported_volumes: Vec<UnsupportedImageVolume>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LvmDiscoverySource {
    pub source_path: PathBuf,
    pub source_kind: DataSourceKind,
}

impl LvmDiscoverySource {
    pub fn new(source_path: impl Into<PathBuf>, source_kind: DataSourceKind) -> Self {
        Self {
            source_path: source_path.into(),
            source_kind,
        }
    }
}
