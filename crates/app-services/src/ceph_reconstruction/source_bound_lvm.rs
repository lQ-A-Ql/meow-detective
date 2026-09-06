use std::io;
use std::path::{Path, PathBuf};

use domain::{DataSourceId, DataSourceKind};
use evidence_core::EvidenceReader;
use persistence_sqlite::repositories::ceph_osd_device_binding_repo::{
    CephOsdDeviceBindingRepo, CephOsdPvBindingRecord, CephOsdSourceBoundDevice,
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeedReassociateReason {
    BindingRegistrationMismatch,
    RegisteredSourcePathMissing,
    RegisteredSourceIdentityChanged,
    PhysicalVolumePathMissing { ordinal: u32 },
    PhysicalVolumeIdentityChanged { ordinal: u32 },
}

impl std::fmt::Display for NeedReassociateReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BindingRegistrationMismatch => {
                write!(
                    formatter,
                    "the persisted binding no longer matches its source record"
                )
            }
            Self::RegisteredSourcePathMissing => {
                write!(formatter, "the registered evidence path is unavailable")
            }
            Self::RegisteredSourceIdentityChanged => {
                write!(formatter, "the registered evidence identity has changed")
            }
            Self::PhysicalVolumePathMissing { ordinal } => {
                write!(formatter, "physical volume {ordinal} is unavailable")
            }
            Self::PhysicalVolumeIdentityChanged { ordinal } => {
                write!(
                    formatter,
                    "physical volume {ordinal} moved or changed identity"
                )
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum SourceBoundLvmError {
    #[error("Ceph OSD device binding was not found for this data source")]
    BindingNotFound,
    #[error("Ceph evidence must be reassociated: {reason}")]
    NeedReassociate { reason: NeedReassociateReason },
    #[error("unsupported source kind in Ceph OSD device binding: {kind}")]
    UnsupportedSourceKind { kind: String },
    #[error("failed to open bound evidence reader for physical volume {ordinal}")]
    EvidenceOpen { ordinal: u32 },
    #[error("LVM physical volume {ordinal} has an invalid label")]
    PhysicalVolumeLabelInvalid { ordinal: u32 },
    #[error("LVM physical volume {ordinal} UUID does not match the persisted binding")]
    PhysicalVolumeUuidMismatch { ordinal: u32 },
    #[error("LVM pool discovery failed")]
    LvmDiscovery,
    #[error("LVM volume group identity does not match the persisted binding")]
    VolumeGroupIdentityMismatch,
    #[error("LVM logical volume identity does not match the persisted binding")]
    LogicalVolumeIdentityMismatch,
    #[error(
        "LVM logical volume size does not match the persisted binding: expected {expected}, found {actual}"
    )]
    DeviceSizeMismatch { expected: u64, actual: u64 },
    #[error("Ceph source-bound device repository failed: {0}")]
    Repository(#[from] persistence_sqlite::DbError),
    #[error("failed to inspect the bound evidence source")]
    SourceIo,
}

impl transport::ServiceErrorCategory for SourceBoundLvmError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::BindingNotFound
            | Self::NeedReassociate { .. }
            | Self::PhysicalVolumeLabelInvalid { .. }
            | Self::PhysicalVolumeUuidMismatch { .. }
            | Self::VolumeGroupIdentityMismatch
            | Self::LogicalVolumeIdentityMismatch
            | Self::DeviceSizeMismatch { .. } => transport::ErrorCategory::Validation,
            Self::UnsupportedSourceKind { .. } => transport::ErrorCategory::Unsupported,
            Self::EvidenceOpen { .. } | Self::Repository(_) | Self::SourceIo => {
                transport::ErrorCategory::Io
            }
            Self::LvmDiscovery => transport::ErrorCategory::Parser,
        }
    }

    fn code(&self) -> Option<&'static str> {
        match self {
            Self::NeedReassociate { .. } => Some("CEPH_EVIDENCE_REASSOCIATION_REQUIRED"),
            Self::BindingNotFound => Some("CEPH_OSD_DEVICE_BINDING_NOT_FOUND"),
            Self::PhysicalVolumeUuidMismatch { .. } => Some("CEPH_LVM_PV_UUID_MISMATCH"),
            Self::VolumeGroupIdentityMismatch => Some("CEPH_LVM_VG_IDENTITY_MISMATCH"),
            Self::LogicalVolumeIdentityMismatch => Some("CEPH_LVM_LV_IDENTITY_MISMATCH"),
            _ => None,
        }
    }

    fn recoverable(&self) -> Option<bool> {
        match self {
            Self::NeedReassociate { .. } => Some(true),
            Self::BindingNotFound
            | Self::PhysicalVolumeUuidMismatch { .. }
            | Self::VolumeGroupIdentityMismatch
            | Self::LogicalVolumeIdentityMismatch
            | Self::DeviceSizeMismatch { .. } => Some(false),
            _ => None,
        }
    }

    fn safe_details(&self) -> Option<serde_json::Value> {
        match self {
            Self::NeedReassociate { reason } => Some(serde_json::json!({
                "reason": reason.to_string()
            })),
            Self::PhysicalVolumeUuidMismatch { ordinal }
            | Self::PhysicalVolumeLabelInvalid { ordinal }
            | Self::EvidenceOpen { ordinal } => Some(serde_json::json!({
                "physicalVolumeOrdinal": ordinal
            })),
            Self::DeviceSizeMismatch { expected, actual } => Some(serde_json::json!({
                "expectedSize": expected,
                "actualSize": actual
            })),
            _ => None,
        }
    }

    fn suggestion(&self) -> Option<&'static str> {
        match self {
            Self::NeedReassociate { .. } => {
                Some("Reassociate the original evidence source before retrying reconstruction.")
            }
            Self::BindingNotFound => Some("Re-import the BlueStore metadata source."),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundEvidenceOpenError {
    pub kind: io::ErrorKind,
}

pub trait SourceBoundEvidenceOpener {
    fn open(
        &self,
        path: &Path,
        kind: &DataSourceKind,
    ) -> Result<Box<dyn EvidenceReader>, BoundEvidenceOpenError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FilesystemEvidenceOpener;

impl SourceBoundEvidenceOpener for FilesystemEvidenceOpener {
    fn open(
        &self,
        path: &Path,
        kind: &DataSourceKind,
    ) -> Result<Box<dyn EvidenceReader>, BoundEvidenceOpenError> {
        match kind {
            DataSourceKind::E01 => image_e01::E01Reader::open(path)
                .map(|reader| Box::new(reader) as Box<dyn EvidenceReader>)
                .map_err(|_| BoundEvidenceOpenError {
                    kind: io::ErrorKind::Other,
                }),
            DataSourceKind::Raw => evidence_core::RawImageReader::open(path)
                .map(|reader| Box::new(reader) as Box<dyn EvidenceReader>)
                .map_err(|error| BoundEvidenceOpenError { kind: error.kind() }),
            DataSourceKind::LocalDisk => evidence_core::LocalDiskReader::open(path)
                .map(|reader| Box::new(reader) as Box<dyn EvidenceReader>)
                .map_err(|error| BoundEvidenceOpenError { kind: error.kind() }),
            DataSourceKind::LogicalDirectory => Err(BoundEvidenceOpenError {
                kind: io::ErrorKind::Unsupported,
            }),
            DataSourceKind::CephRbd | DataSourceKind::CephFs => Err(BoundEvidenceOpenError {
                kind: io::ErrorKind::Unsupported,
            }),
        }
    }
}

pub fn open_source_bound_bluestore_lvm(
    source_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    inventory_id: &str,
) -> Result<Box<dyn EvidenceReader>, SourceBoundLvmError> {
    open_source_bound_bluestore_lvm_with_opener(
        source_conn,
        data_source_id,
        inventory_id,
        &FilesystemEvidenceOpener,
    )
}

pub(crate) fn open_source_bound_bluestore_lvm_for_case(
    source_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    inventory_id: &str,
    case_id: &str,
) -> Result<Box<dyn EvidenceReader>, SourceBoundLvmError> {
    open_source_bound_bluestore_lvm_with_opener(
        source_conn,
        data_source_id,
        inventory_id,
        &CaseScopedFilesystemEvidenceOpener { case_id },
    )
}

struct CaseScopedFilesystemEvidenceOpener<'a> {
    case_id: &'a str,
}

impl SourceBoundEvidenceOpener for CaseScopedFilesystemEvidenceOpener<'_> {
    fn open(
        &self,
        path: &Path,
        kind: &DataSourceKind,
    ) -> Result<Box<dyn EvidenceReader>, BoundEvidenceOpenError> {
        match kind {
            DataSourceKind::E01 => {
                crate::e01_reader_cache::open_e01_reader_cached(path, self.case_id)
                    .map(|reader| Box::new(reader) as Box<dyn EvidenceReader>)
                    .map_err(|_| BoundEvidenceOpenError {
                        kind: io::ErrorKind::Other,
                    })
            }
            DataSourceKind::Raw => evidence_core::RawImageReader::open(path)
                .map(|reader| Box::new(reader) as Box<dyn EvidenceReader>)
                .map_err(|error| BoundEvidenceOpenError { kind: error.kind() }),
            DataSourceKind::LocalDisk => evidence_core::LocalDiskReader::open(path)
                .map(|reader| Box::new(reader) as Box<dyn EvidenceReader>)
                .map_err(|error| BoundEvidenceOpenError { kind: error.kind() }),
            DataSourceKind::LogicalDirectory | DataSourceKind::CephRbd | DataSourceKind::CephFs => {
                Err(BoundEvidenceOpenError {
                    kind: io::ErrorKind::Unsupported,
                })
            }
        }
    }
}

pub(crate) fn open_source_bound_bluestore_lvm_with_opener(
    source_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    inventory_id: &str,
    opener: &dyn SourceBoundEvidenceOpener,
) -> Result<Box<dyn EvidenceReader>, SourceBoundLvmError> {
    let bound = CephOsdDeviceBindingRepo::new(source_conn)
        .find_source_bound_device(&data_source_id.0, inventory_id)?
        .ok_or(SourceBoundLvmError::BindingNotFound)?;
    validate_registered_source(&bound)?;

    let registered_canonical = canonicalize_bound_path(
        Path::new(&bound.source.source_path),
        NeedReassociateReason::RegisteredSourcePathMissing,
    )?;
    validate_registered_canonical_identity(&bound, &registered_canonical)?;

    let mut readers = Vec::with_capacity(bound.binding.physical_volumes.len());
    let mut offsets = Vec::with_capacity(bound.binding.physical_volumes.len());
    let mut primary_source_seen = false;
    for pv in &bound.binding.physical_volumes {
        let canonical = canonicalize_bound_path(
            Path::new(&pv.source_path),
            NeedReassociateReason::PhysicalVolumePathMissing {
                ordinal: pv.ordinal,
            },
        )?;
        if !paths_match(&canonical, Path::new(&pv.canonical_source_path)) {
            return Err(SourceBoundLvmError::NeedReassociate {
                reason: NeedReassociateReason::PhysicalVolumeIdentityChanged {
                    ordinal: pv.ordinal,
                },
            });
        }
        primary_source_seen |= paths_match(&canonical, &registered_canonical);
        readers.push(open_validated_pv(pv, opener)?);
        offsets.push(pv.pv_offset);
    }
    if !primary_source_seen {
        return Err(SourceBoundLvmError::NeedReassociate {
            reason: NeedReassociateReason::BindingRegistrationMismatch,
        });
    }

    let pool = fs_lvm::LvmPool::discover(readers, offsets)
        .map_err(|_| SourceBoundLvmError::LvmDiscovery)?;
    validate_volume_group(&pool, &bound)?;
    open_bound_logical_volume(&pool, &bound)
}

fn validate_registered_source(bound: &CephOsdSourceBoundDevice) -> Result<(), SourceBoundLvmError> {
    let device = &bound.binding.device;
    if device.data_source_id != bound.source.data_source_id
        || device.source_path != bound.source.source_path
        || device.source_kind != bound.source.source_kind
        || bound.source.canonical_source_path.as_deref()
            != Some(device.canonical_source_path.as_str())
    {
        return Err(SourceBoundLvmError::NeedReassociate {
            reason: NeedReassociateReason::BindingRegistrationMismatch,
        });
    }
    Ok(())
}

fn validate_registered_canonical_identity(
    bound: &CephOsdSourceBoundDevice,
    actual: &Path,
) -> Result<(), SourceBoundLvmError> {
    let expected = Path::new(&bound.binding.device.canonical_source_path);
    if !paths_match(actual, expected) {
        return Err(SourceBoundLvmError::NeedReassociate {
            reason: NeedReassociateReason::RegisteredSourceIdentityChanged,
        });
    }
    Ok(())
}

fn open_validated_pv(
    pv: &CephOsdPvBindingRecord,
    opener: &dyn SourceBoundEvidenceOpener,
) -> Result<Box<dyn EvidenceReader>, SourceBoundLvmError> {
    let kind = parse_source_kind(&pv.source_kind)?;
    let mut reader = opener
        .open(Path::new(&pv.source_path), &kind)
        .map_err(|_| SourceBoundLvmError::EvidenceOpen {
            ordinal: pv.ordinal,
        })?;
    let label = fs_lvm::label::parse_pv_label(reader.as_mut(), pv.pv_offset).map_err(|_| {
        SourceBoundLvmError::PhysicalVolumeLabelInvalid {
            ordinal: pv.ordinal,
        }
    })?;
    if normalize_lvm_uuid(&label.pv_uuid) != normalize_lvm_uuid(&pv.pv_uuid) {
        return Err(SourceBoundLvmError::PhysicalVolumeUuidMismatch {
            ordinal: pv.ordinal,
        });
    }
    Ok(reader)
}

fn validate_volume_group(
    pool: &fs_lvm::LvmPool,
    bound: &CephOsdSourceBoundDevice,
) -> Result<(), SourceBoundLvmError> {
    let actual = pool.volume_group();
    let expected = &bound.binding.device;
    if normalize_lvm_uuid(&actual.id) != normalize_lvm_uuid(&expected.lvm_vg_uuid)
        || actual.name != expected.lvm_vg_name
    {
        return Err(SourceBoundLvmError::VolumeGroupIdentityMismatch);
    }
    Ok(())
}

fn open_bound_logical_volume(
    pool: &fs_lvm::LvmPool,
    bound: &CephOsdSourceBoundDevice,
) -> Result<Box<dyn EvidenceReader>, SourceBoundLvmError> {
    let expected = &bound.binding.device;
    let volumes = pool.list_volumes();
    let index = volumes
        .iter()
        .position(|volume| {
            normalize_lvm_uuid(&volume.uuid) == normalize_lvm_uuid(&expected.lvm_lv_uuid)
        })
        .ok_or(SourceBoundLvmError::LogicalVolumeIdentityMismatch)?;
    if volumes[index].name != expected.lvm_lv_name {
        return Err(SourceBoundLvmError::LogicalVolumeIdentityMismatch);
    }
    let reader = pool
        .open_volume_reader(index)
        .map_err(|_| SourceBoundLvmError::LogicalVolumeIdentityMismatch)?;
    let actual_size = reader.info().size;
    if actual_size != expected.device_size {
        return Err(SourceBoundLvmError::DeviceSizeMismatch {
            expected: expected.device_size,
            actual: actual_size,
        });
    }
    Ok(reader)
}

fn canonicalize_bound_path(
    path: &Path,
    missing_reason: NeedReassociateReason,
) -> Result<PathBuf, SourceBoundLvmError> {
    std::fs::canonicalize(path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            SourceBoundLvmError::NeedReassociate {
                reason: missing_reason,
            }
        } else {
            SourceBoundLvmError::SourceIo
        }
    })
}

fn parse_source_kind(value: &str) -> Result<DataSourceKind, SourceBoundLvmError> {
    match value {
        "e01" => Ok(DataSourceKind::E01),
        "raw" => Ok(DataSourceKind::Raw),
        "local_disk" => Ok(DataSourceKind::LocalDisk),
        "ceph_rbd" => Ok(DataSourceKind::CephRbd),
        _ => Err(SourceBoundLvmError::UnsupportedSourceKind {
            kind: value.to_string(),
        }),
    }
}

fn normalize_lvm_uuid(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|character| *character != '-')
        .collect::<String>()
        .to_ascii_lowercase()
}

fn paths_match(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

#[cfg(test)]
#[path = "../../tests/unit/ceph_reconstruction/source_bound_lvm.rs"]
mod tests;
