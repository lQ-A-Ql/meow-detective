use domain::DataSourceKind;
use fs_lvm::mdraid::MdRaidSuperblock;
use sha2::Digest;
use thiserror::Error;

use crate::datasource_service::{self, LvmLogicalVolumeIdentity};

mod content;
pub(crate) mod discovery;
mod filesystem;
mod identity;
mod native_version;
pub(crate) mod persisted_inputs;

use crate::source_runtime::content::SourceContent;
pub use content::{mdraid_reader_content_bytes, verify_mdraid_source_content};
pub use discovery::discover_linux_evidence_set_mdraid_with_cancel;
pub use discovery::discover_mdraid_from_collection;
pub use discovery::{discover_linux_evidence_set_mdraid, MdRaidDiscoveryMember};
pub(crate) use filesystem::open_mdraid_partition;
pub use filesystem::{open_mdraid_filesystem, probe_mdraid_filesystems};
pub use identity::registered_source_id;
pub use native_version::mdraid_native_version;

#[derive(Debug, Error)]
pub enum MdRaidPlanError {
    #[error("evidence source could not be opened: {0}")]
    Io(#[from] std::io::Error),
    #[error("LVM metadata could not be parsed: {0}")]
    Lvm(#[from] fs_lvm::LvmError),
    #[error("evidence source classification failed: {0}")]
    Classification(#[from] datasource_service::DataSourceError),
    #[error("MD RAID reader could not be opened: {0}")]
    MdRaid(#[from] fs_lvm::mdraid::MdRaidError),
    #[error("MD RAID descriptor serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("MD RAID catalog database operation failed: {0}")]
    Database(#[from] persistence_sqlite::DbError),
    #[error("MD RAID catalog storage operation failed: {0}")]
    Storage(#[from] persistence_source::runtime::StorageRuntimeError),
    #[error("MD RAID catalog generation failed: {0}")]
    Generation(#[from] persistence_source::fjall::FjallStorageError),
    #[error("MD RAID catalog build is already active")]
    Busy,
    #[error("MD RAID catalog publication is inconsistent")]
    InvalidPublication,
    #[error("MD RAID source content identity is missing or invalid")]
    InvalidContentIdentity,
    #[error("MD RAID source content changed since discovery")]
    SourceContentChanged,
    #[error("MD RAID catalog materialization cancelled")]
    Cancelled,
    #[error("collection image plan failed: {0}")]
    Collection(#[from] datasource_service::CollectionPlanError),
}

impl transport::ServiceErrorCategory for MdRaidPlanError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Io(_) | Self::Lvm(_) | Self::Database(_) | Self::Storage(_) => {
                transport::ErrorCategory::Io
            }
            Self::MdRaid(_) => transport::ErrorCategory::Unsupported,
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

impl From<rusqlite::Error> for MdRaidPlanError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MdRaidMemberPlan {
    pub superblock: MdRaidSuperblock,
    pub origin: MdRaidMemberOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MdRaidMemberOrigin {
    Lvm {
        vg_name: String,
        vg_uuid: String,
        lv_name: String,
        lv_uuid: String,
        identity: LvmLogicalVolumeIdentity,
    },
    Direct {
        source_path: String,
        source_kind: DataSourceKind,
        byte_offset: u64,
        byte_length: u64,
    },
}

impl MdRaidMemberOrigin {
    pub fn label(&self) -> String {
        match self {
            Self::Lvm {
                vg_name, lv_name, ..
            } => format!("{vg_name}/{lv_name}"),
            Self::Direct {
                source_path,
                byte_offset,
                ..
            } => format!(
                "{} @ {byte_offset}",
                std::path::Path::new(source_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(source_path)
            ),
        }
    }

    pub fn vg_name(&self) -> String {
        match self {
            Self::Lvm { vg_name, .. } => vg_name.clone(),
            Self::Direct { .. } => String::new(),
        }
    }

    pub fn lv_name(&self) -> String {
        match self {
            Self::Lvm { lv_name, .. } => lv_name.clone(),
            Self::Direct { .. } => self.label(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MdRaidArrayPlan {
    pub array_uuid: [u8; 16],
    pub name: String,
    pub level: i32,
    pub layout: u32,
    pub chunk_sectors: u32,
    pub raid_disks: u32,
    pub min_data_size_sectors: u64,
    pub capacity_mismatch: bool,
    pub source_snapshot: crate::source_runtime::snapshot::SourceSnapshot,
    pub source_content: Vec<SourceContent>,
    pub members: Vec<MdRaidMemberPlan>,
}

pub fn open_mdraid_reader(
    plan: &MdRaidArrayPlan,
) -> Result<fs_lvm::mdraid::MdRaidReader, MdRaidPlanError> {
    validate_plan_shape(plan)?;
    validate_source_snapshot(plan)?;
    content::validate_content_bindings(plan)?;
    let mut members = Vec::with_capacity(plan.members.len());
    for member in &plan.members {
        let mut reader = open_member_reader(plan, &member.origin)?;
        let size = reader.info().size;
        let current = fs_lvm::mdraid::probe_superblock(&mut *reader, size)
            .ok_or_else(|| {
                fs_lvm::mdraid::MdRaidError::MetadataConflict(
                    "member metadata is no longer readable".to_string(),
                )
            })?
            .superblock;
        if current != member.superblock {
            return Err(fs_lvm::mdraid::MdRaidError::MetadataConflict(
                "member metadata changed since the read plan was created".to_string(),
            )
            .into());
        }
        members.push(fs_lvm::mdraid::MdRaidMember::new(
            member.superblock.clone(),
            reader,
        ));
    }
    Ok(fs_lvm::mdraid::MdRaidReader::open(members)?)
}

fn validate_source_snapshot(plan: &MdRaidArrayPlan) -> Result<(), MdRaidPlanError> {
    if plan.source_snapshot.is_empty() {
        return Err(fs_lvm::mdraid::MdRaidError::MetadataConflict(
            "MD RAID source snapshot is missing".to_string(),
        )
        .into());
    }
    plan.source_snapshot.validate()?;
    if content::input_sources(plan)?
        .keys()
        .any(|path| !plan.source_snapshot.contains(std::path::Path::new(path)))
    {
        return Err(fs_lvm::mdraid::MdRaidError::MetadataConflict(
            "MD RAID source snapshot does not cover all inputs".to_string(),
        )
        .into());
    }
    Ok(())
}

pub fn mdraid_array_fingerprint(plan: &MdRaidArrayPlan) -> Result<String, MdRaidPlanError> {
    validate_plan_shape(plan)?;
    let serialized = serde_json::to_vec(plan)?;
    Ok(hex::encode(sha2::Sha256::digest(serialized)))
}

fn open_member_reader(
    plan: &MdRaidArrayPlan,
    origin: &MdRaidMemberOrigin,
) -> Result<Box<dyn evidence_core::EvidenceReader>, MdRaidPlanError> {
    match origin {
        MdRaidMemberOrigin::Lvm {
            lv_uuid,
            lv_name,
            identity,
            ..
        } => {
            let pool = crate::lvm::mapping_cache::open(
                &identity.pv_sources,
                |source| {
                    plan.source_content
                        .iter()
                        .find(|known| {
                            known.source_path == source.source_path
                                && source.source_kind.as_ref() == Some(&known.source_kind)
                        })
                        .cloned()
                        .ok_or_else(|| std::io::Error::other("MD LVM source content is missing"))
                },
                &std::sync::atomic::AtomicBool::new(false),
                None,
            )?;
            if identity.vg_uuid.is_empty() || pool.volume_group().id != identity.vg_uuid {
                return Err(fs_lvm::mdraid::MdRaidError::MetadataConflict(
                    "logical volume group identity does not match the read plan".to_string(),
                )
                .into());
            }
            let volumes = pool.list_volumes();
            let index = volumes
                .iter()
                .position(|volume| !lv_uuid.is_empty() && volume.uuid == *lv_uuid)
                .ok_or_else(|| {
                    MdRaidPlanError::Classification(datasource_service::DataSourceError::Evidence(
                        format!("LVM logical volume '{lv_name}' was not found"),
                    ))
                })?;
            Ok(pool.open_volume(index)?)
        }
        MdRaidMemberOrigin::Direct {
            source_path,
            source_kind,
            byte_offset,
            byte_length,
        } => {
            let reader = content::open_verified_source(
                plan,
                std::path::Path::new(source_path),
                source_kind,
            )?;
            Ok(Box::new(evidence_core::PartitionWindowReader::new(
                reader,
                *byte_offset,
                Some(*byte_length),
            )?))
        }
    }
}

pub fn mdraid_descriptor_path(
    case_root: &std::path::Path,
    array_uuid: &[u8; 16],
    fingerprint: &str,
) -> std::path::PathBuf {
    case_root
        .join("derived")
        .join("mdraid")
        .join(format!("{}-{fingerprint}.json", hex::encode(array_uuid)))
}

pub fn write_mdraid_descriptor(
    case_root: &std::path::Path,
    plan: &MdRaidArrayPlan,
) -> Result<std::path::PathBuf, MdRaidPlanError> {
    validate_plan_shape(plan)?;
    let fingerprint = mdraid_array_fingerprint(plan)?;
    let target = mdraid_descriptor_path(case_root, &plan.array_uuid, &fingerprint);
    let parent = target
        .parent()
        .ok_or_else(|| std::io::Error::other("MD RAID descriptor has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let payload = serde_json::to_vec_pretty(plan)?;
    if target.exists() {
        if std::fs::read(&target)? != payload {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "MD RAID descriptor identity conflicts with its contents",
            )
            .into());
        }
        return Ok(target);
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut temporary, &payload)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(&target)
        .map_err(|error| error.error)?;
    Ok(target)
}

pub fn open_persisted_mdraid_reader(
    descriptor_path: &std::path::Path,
) -> Result<fs_lvm::mdraid::MdRaidReader, MdRaidPlanError> {
    open_mdraid_reader(&read_mdraid_descriptor(descriptor_path)?)
}

pub fn read_mdraid_descriptor(
    descriptor_path: &std::path::Path,
) -> Result<MdRaidArrayPlan, MdRaidPlanError> {
    let payload = std::fs::read(descriptor_path)?;
    let plan: MdRaidArrayPlan = serde_json::from_slice(&payload)?;
    let expected = format!(
        "{}-{}.json",
        hex::encode(plan.array_uuid),
        mdraid_array_fingerprint(&plan)?,
    );
    if descriptor_path.file_name().and_then(|name| name.to_str()) != Some(expected.as_str()) {
        return Err(MdRaidPlanError::InvalidContentIdentity);
    }
    Ok(plan)
}

fn validate_plan_shape(plan: &MdRaidArrayPlan) -> Result<(), MdRaidPlanError> {
    if plan.raid_disks == 0 || plan.members.is_empty() {
        return Err(MdRaidPlanError::MdRaid(
            fs_lvm::mdraid::MdRaidError::EmptyMembers,
        ));
    }
    let mut slots = std::collections::BTreeSet::new();
    for member in &plan.members {
        let metadata = &member.superblock;
        if metadata.array_uuid != plan.array_uuid
            || metadata.level != plan.level
            || metadata.layout != plan.layout
            || metadata.chunk_sectors != plan.chunk_sectors
            || metadata.raid_disks != plan.raid_disks
        {
            return Err(fs_lvm::mdraid::MdRaidError::MetadataConflict(
                "MD RAID plan geometry differs from its member metadata".to_string(),
            )
            .into());
        }
        let slot = member.superblock.slot_index().ok_or_else(|| {
            MdRaidPlanError::MdRaid(fs_lvm::mdraid::MdRaidError::MetadataConflict(
                "member has no active MD RAID slot".into(),
            ))
        })?;
        if slot >= plan.raid_disks {
            return Err(fs_lvm::mdraid::MdRaidError::InvalidDeviceCount.into());
        }
        if !slots.insert(slot) {
            return Err(MdRaidPlanError::MdRaid(
                fs_lvm::mdraid::MdRaidError::MetadataConflict(format!(
                    "multiple devices claim RAID slot {slot}"
                )),
            ));
        }
    }
    let minimum = match plan.level {
        -1 | 0 => plan.raid_disks as usize,
        1 => 1,
        4 | 5 => plan.raid_disks.saturating_sub(1) as usize,
        6 => plan.raid_disks.saturating_sub(2) as usize,
        10 => 1,
        level => {
            return Err(MdRaidPlanError::MdRaid(
                fs_lvm::mdraid::MdRaidError::UnsupportedLevel(level),
            ))
        }
    };
    if slots.len() < minimum {
        return Err(MdRaidPlanError::MdRaid(
            fs_lvm::mdraid::MdRaidError::MemberCount {
                expected: plan.raid_disks,
                actual: slots.len(),
            },
        ));
    }
    Ok(())
}
