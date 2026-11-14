use domain::DataSourceKind;
use fs_lvm::mdraid::MdRaidSuperblock;
use thiserror::Error;

use crate::datasource_service::{self, LvmLogicalVolumeIdentity};

mod discovery;

pub use discovery::{discover_linux_evidence_set_mdraid, MdRaidDiscoveryMember};

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
    #[error("MD RAID catalog materialization cancelled")]
    Cancelled,
}

impl transport::ServiceErrorCategory for MdRaidPlanError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Io(_) | Self::Lvm(_) => transport::ErrorCategory::Io,
            Self::MdRaid(_) => transport::ErrorCategory::Unsupported,
            Self::Classification(error) => error.category(),
            Self::Json(_) => transport::ErrorCategory::Parser,
            Self::Cancelled => transport::ErrorCategory::Cancelled,
        }
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
    pub members: Vec<MdRaidMemberPlan>,
}

pub fn open_mdraid_reader(
    plan: &MdRaidArrayPlan,
) -> Result<fs_lvm::mdraid::MdRaidReader, MdRaidPlanError> {
    let mut members = Vec::with_capacity(plan.members.len());
    for member in &plan.members {
        let reader = open_member_reader(&member.origin)?;
        members.push(fs_lvm::mdraid::MdRaidMember::new(
            member.superblock.clone(),
            reader,
        ));
    }
    Ok(fs_lvm::mdraid::MdRaidReader::open(members)?)
}

fn open_member_reader(
    origin: &MdRaidMemberOrigin,
) -> Result<Box<dyn evidence_core::EvidenceReader>, MdRaidPlanError> {
    match origin {
        MdRaidMemberOrigin::Lvm {
            lv_uuid,
            lv_name,
            identity,
            ..
        } => {
            let readers = identity
                .pv_sources
                .iter()
                .map(|source| {
                    let kind = source.source_kind.as_ref().ok_or_else(|| {
                        MdRaidPlanError::Classification(
                            datasource_service::DataSourceError::Evidence(
                                "LVM PV source kind is missing".to_string(),
                            ),
                        )
                    })?;
                    datasource_service::open_evidence_reader(
                        std::path::Path::new(&source.source_path),
                        kind,
                    )
                    .map_err(MdRaidPlanError::Io)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let pool = fs_lvm::LvmPool::discover(readers, identity.pv_offsets.clone())?;
            let volumes = pool.list_volumes();
            let index = volumes
                .iter()
                .position(|volume| {
                    (!lv_uuid.is_empty() && volume.uuid == *lv_uuid)
                        || (lv_uuid.is_empty() && volume.name == *lv_name)
                })
                .ok_or_else(|| {
                    MdRaidPlanError::Classification(datasource_service::DataSourceError::Evidence(
                        format!("LVM logical volume '{lv_name}' was not found"),
                    ))
                })?;
            Ok(pool.open_volume_reader(index)?)
        }
        MdRaidMemberOrigin::Direct {
            source_path,
            source_kind,
            byte_offset,
            byte_length,
        } => {
            let reader = datasource_service::open_evidence_reader(
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
) -> std::path::PathBuf {
    case_root
        .join("derived")
        .join("mdraid")
        .join(format!("{}.json", hex::encode(array_uuid)))
}

pub fn write_mdraid_descriptor(
    case_root: &std::path::Path,
    plan: &MdRaidArrayPlan,
) -> Result<std::path::PathBuf, MdRaidPlanError> {
    validate_plan_shape(plan)?;
    let path = mdraid_descriptor_path(case_root, &plan.array_uuid);
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("MD RAID descriptor has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, serde_json::to_vec_pretty(plan)?)?;
    if let Err(error) = std::fs::rename(&temporary, &path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(path)
}

pub fn open_persisted_mdraid_reader(
    descriptor_path: &std::path::Path,
) -> Result<fs_lvm::mdraid::MdRaidReader, MdRaidPlanError> {
    let payload = std::fs::read(descriptor_path)?;
    let plan: MdRaidArrayPlan = serde_json::from_slice(&payload)?;
    open_mdraid_reader(&plan)
}

fn validate_plan_shape(plan: &MdRaidArrayPlan) -> Result<(), MdRaidPlanError> {
    if plan.raid_disks == 0 || plan.members.is_empty() {
        return Err(MdRaidPlanError::MdRaid(
            fs_lvm::mdraid::MdRaidError::EmptyMembers,
        ));
    }
    let mut slots = std::collections::BTreeSet::new();
    for member in &plan.members {
        let slot = member.superblock.slot_index().ok_or_else(|| {
            MdRaidPlanError::MdRaid(fs_lvm::mdraid::MdRaidError::MetadataConflict(
                "member has no active MD RAID slot".into(),
            ))
        })?;
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
