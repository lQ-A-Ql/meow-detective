use domain::structure::NativeVersion;
use fs_lvm::{LvMeta, SegmentArea, SegmentType, VolumeGroup};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::source_runtime::content::SourceContent;
use crate::source_runtime::snapshot::SourceSnapshot;

pub const LVM_ADAPTER_VERSION: &str = "fs-lvm/lv-v1";
pub const LVM_POLICY_VERSION: &str = "lvm-lv-policy-v1";
const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LvmVolumePlan {
    pub schema_version: u32,
    pub vg_uuid: String,
    pub vg_name: String,
    pub vg_seqno: u64,
    pub vg_extent_size_sectors: u64,
    pub lv_uuid: String,
    pub lv_name: String,
    pub logical_size: u64,
    pub segment_sha256: String,
    pub sources: Vec<LvmVolumeSourcePlan>,
    pub source_snapshot: SourceSnapshot,
    pub adapter_version: String,
    pub policy_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LvmVolumeSourcePlan {
    pub source_path: String,
    pub source_kind: domain::DataSourceKind,
    pub pv_uuid: String,
    pub offset: u64,
    pub content: SourceContent,
}

impl LvmVolumePlan {
    pub fn schema_version() -> u32 {
        SCHEMA_VERSION
    }

    pub fn validate(&self) -> Result<(), LvmPlanError> {
        if self.schema_version != SCHEMA_VERSION
            || self.vg_uuid.is_empty()
            || self.lv_uuid.is_empty()
            || self.lv_name.is_empty()
            || self.logical_size == 0
            || self.sources.is_empty()
            || self.adapter_version != LVM_ADAPTER_VERSION
            || self.policy_version != LVM_POLICY_VERSION
            || !valid_digest(&self.segment_sha256)
        {
            return Err(LvmPlanError::InvalidContentIdentity);
        }
        for source in &self.sources {
            source
                .content
                .validate()
                .map_err(|_| LvmPlanError::InvalidContentIdentity)?;
            if source.pv_uuid.is_empty() || source.source_path.is_empty() {
                return Err(LvmPlanError::InvalidContentIdentity);
            }
        }
        Ok(())
    }
}

pub fn lvm_volume_fingerprint(plan: &LvmVolumePlan) -> Result<String, LvmPlanError> {
    plan.validate()?;
    let serialized = serde_json::to_vec(plan)?;
    Ok(hex::encode(Sha256::digest(serialized)))
}

pub fn lvm_native_version(plan: &LvmVolumePlan) -> NativeVersion {
    NativeVersion::LvmCommitted {
        vg_uuid: plan.vg_uuid.clone(),
        seqno: plan.vg_seqno,
    }
}

pub(crate) fn segment_digest(
    volume_group: &VolumeGroup,
    volume: &LvMeta,
) -> Result<String, LvmPlanError> {
    if volume.segments.is_empty() {
        return Err(LvmPlanError::InvalidContentIdentity);
    }
    let mut digest = Sha256::new();
    digest.update(b"lvm-segments-v1");
    for value in [
        volume_group.id.as_str(),
        volume_group.name.as_str(),
        volume.uuid.as_str(),
        volume.name.as_str(),
        volume.role.as_str(),
    ] {
        digest_value(&mut digest, value.as_bytes());
    }
    digest.update(volume_group.extent_size.to_le_bytes());
    digest.update((volume.segments.len() as u64).to_le_bytes());
    for segment in &volume.segments {
        digest.update(segment.start_extent.to_le_bytes());
        digest.update(segment.extent_count.to_le_bytes());
        digest_value(
            &mut digest,
            segment_type_label(&segment.seg_type).as_bytes(),
        );
        digest.update((segment.areas.len() as u64).to_le_bytes());
        for area in &segment.areas {
            let (kind, name, start) = match area {
                SegmentArea::PhysicalVolume { name, start_extent } => {
                    ("pv", name.as_str(), *start_extent)
                }
                SegmentArea::LogicalVolume { name, start_extent } => {
                    ("lv", name.as_str(), *start_extent)
                }
                SegmentArea::Unassigned { start_extent } => ("unassigned", "", *start_extent),
            };
            digest_value(&mut digest, kind.as_bytes());
            digest_value(&mut digest, name.as_bytes());
            digest.update(start.to_le_bytes());
        }
    }
    Ok(hex::encode(digest.finalize()))
}

fn segment_type_label(segment: &SegmentType) -> String {
    match segment {
        SegmentType::Linear => "linear".into(),
        SegmentType::Striped {
            stripe_count,
            stripe_size,
        } => format!("striped:{stripe_count}:{stripe_size}"),
        SegmentType::Raid0 {
            stripe_count,
            stripe_size,
        } => format!("raid0:{stripe_count}:{stripe_size}"),
        SegmentType::Raid1 { mirror_count } => format!("raid1:{mirror_count}"),
        SegmentType::Raid5 { stripe_count } => format!("raid5:{stripe_count}"),
        SegmentType::Raid6 { stripe_count } => format!("raid6:{stripe_count}"),
        SegmentType::Raid10 {
            stripe_count,
            mirror_count,
        } => format!("raid10:{stripe_count}:{mirror_count}"),
        SegmentType::ThinVolume => "thin".into(),
        SegmentType::ThinPool => "thin-pool".into(),
        SegmentType::Snapshot => "snapshot".into(),
        SegmentType::CacheVolume => "cache".into(),
        SegmentType::CachePool => "cache-pool".into(),
        SegmentType::Unsupported { type_name } => format!("unsupported:{type_name}"),
    }
}

fn digest_value(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_le_bytes());
    digest.update(value);
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Debug, thiserror::Error)]
pub enum LvmPlanError {
    #[error("evidence source could not be opened: {0}")]
    Io(#[from] std::io::Error),
    #[error("LVM metadata could not be parsed: {0}")]
    Lvm(#[from] fs_lvm::LvmError),
    #[error("evidence source classification failed: {0}")]
    Classification(#[from] crate::datasource_service::DataSourceError),
    #[error("LVM descriptor serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("LVM catalog database operation failed: {0}")]
    Database(#[from] persistence_sqlite::DbError),
    #[error("LVM catalog storage operation failed: {0}")]
    Storage(#[from] persistence_source::runtime::StorageRuntimeError),
    #[error("LVM catalog generation failed: {0}")]
    Generation(#[from] persistence_source::fjall::FjallStorageError),
    #[error("LVM catalog build is already active")]
    Busy,
    #[error("LVM catalog publication is inconsistent")]
    InvalidPublication,
    #[error("LVM source content identity is missing or invalid")]
    InvalidContentIdentity,
    #[error("LVM source content changed since discovery")]
    SourceContentChanged,
    #[error("LVM catalog materialization cancelled")]
    Cancelled,
    #[error("collection image plan failed: {0}")]
    Collection(#[from] crate::datasource_service::CollectionPlanError),
}

impl From<crate::source_runtime::content::ContentError> for LvmPlanError {
    fn from(error: crate::source_runtime::content::ContentError) -> Self {
        match error {
            crate::source_runtime::content::ContentError::Cancelled => Self::Cancelled,
            crate::source_runtime::content::ContentError::Changed => Self::SourceContentChanged,
            crate::source_runtime::content::ContentError::InvalidIdentity => {
                Self::InvalidContentIdentity
            }
            crate::source_runtime::content::ContentError::Io(error) => Self::Io(error),
            other => Self::Io(std::io::Error::other(other)),
        }
    }
}

impl transport::ServiceErrorCategory for LvmPlanError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Io(_) | Self::Lvm(_) | Self::Database(_) | Self::Storage(_) => {
                transport::ErrorCategory::Io
            }
            Self::Classification(error) => error.category(),
            Self::Collection(error) => error.category(),
            Self::Json(_) => transport::ErrorCategory::Parser,
            Self::Cancelled => transport::ErrorCategory::Cancelled,
            Self::Generation(_)
            | Self::InvalidPublication
            | Self::InvalidContentIdentity
            | Self::SourceContentChanged => transport::ErrorCategory::Parser,
            Self::Busy => transport::ErrorCategory::Validation,
        }
    }
}

impl From<rusqlite::Error> for LvmPlanError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error.into())
    }
}
