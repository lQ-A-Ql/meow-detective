use std::collections::BTreeMap;

use domain::DataSourceKind;
use fs_lvm::mdraid::{read_superblock, MdRaidSuperblock};
use thiserror::Error;

use crate::datasource_service::{self, LvmLogicalVolumeIdentity, LvmPhysicalVolumeSource};

use super::linux_import::LinuxEvidenceSetImportPlan;

const MD_SUPERBLOCK_LV_OFFSET: u64 = 4096;

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
}

impl transport::ServiceErrorCategory for MdRaidPlanError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Io(_) | Self::Lvm(_) => transport::ErrorCategory::Io,
            Self::MdRaid(_) => transport::ErrorCategory::Unsupported,
            Self::Classification(error) => error.category(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdRaidMemberPlan {
    pub superblock: MdRaidSuperblock,
    pub vg_name: String,
    pub vg_uuid: String,
    pub lv_name: String,
    pub lv_uuid: String,
    pub lvm_identity: LvmLogicalVolumeIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug)]
struct PvEvidence {
    source: LvmPhysicalVolumeSource,
    label: fs_lvm::LvmLabel,
    volume_group: fs_lvm::VolumeGroup,
}

pub fn discover_linux_evidence_set_mdraid(
    plan: &LinuxEvidenceSetImportPlan,
) -> Result<Vec<MdRaidArrayPlan>, MdRaidPlanError> {
    let mut pvs = Vec::new();
    for member in &plan.members {
        let mut reader =
            datasource_service::open_evidence_reader(&member.source_path, &member.source_kind)?;
        let probe = datasource_service::detect_image_filesystem(&mut reader)?;
        for candidate in probe
            .candidates
            .iter()
            .filter(|candidate| candidate.kind == datasource_service::ImageFilesystemKind::LvmPool)
        {
            let mut source_reader =
                datasource_service::open_evidence_reader(&member.source_path, &member.source_kind)?;
            if let Some(evidence) = inspect_pv(
                &mut source_reader,
                &member.source_path,
                &member.source_kind,
                candidate.offset,
            )? {
                if !pvs.iter().any(|known: &PvEvidence| {
                    known.source.offset == evidence.source.offset
                        && known.source.source_path == evidence.source.source_path
                }) {
                    pvs.push(evidence);
                }
            }
        }
    }

    let mut groups = BTreeMap::<String, Vec<PvEvidence>>::new();
    for pv in pvs {
        let key = normalized_vg_key(&pv.volume_group);
        groups.entry(key).or_default().push(pv);
    }

    let mut arrays = BTreeMap::<[u8; 16], MdRaidArrayPlan>::new();
    for evidence in groups.into_values() {
        let Some(pool) = open_complete_pool(&evidence)? else {
            continue;
        };
        let sources = pool_sources(&evidence, &pool);
        for (index, volume) in pool.list_volumes().iter().enumerate() {
            let mut reader = pool.open_volume_reader(index)?;
            let Ok(superblock) = read_superblock(&mut *reader, MD_SUPERBLOCK_LV_OFFSET) else {
                continue;
            };
            if superblock.raid_disks == 0 || !superblock.is_raid0() {
                continue;
            }
            let identity = LvmLogicalVolumeIdentity {
                vg_uuid: pool.volume_group().id.clone(),
                vg_name: pool.volume_group().name.clone(),
                lv_uuid: volume.uuid.clone(),
                lv_name: volume.name.clone(),
                pv_offsets: pool
                    .physical_volume_offsets()
                    .iter()
                    .map(|(_, offset)| *offset)
                    .collect(),
                pv_sources: sources.clone(),
            };
            let member = MdRaidMemberPlan {
                superblock: superblock.clone(),
                vg_name: pool.volume_group().name.clone(),
                vg_uuid: pool.volume_group().id.clone(),
                lv_name: volume.name.clone(),
                lv_uuid: volume.uuid.clone(),
                lvm_identity: identity,
            };
            insert_member(&mut arrays, member);
        }
    }

    let mut plans = arrays.into_values().collect::<Vec<_>>();
    plans.retain(|plan| plan.members.len() <= plan.raid_disks as usize);
    for plan in &mut plans {
        plan.members
            .sort_by_key(|member| member.superblock.dev_number);
        plan.min_data_size_sectors = plan
            .members
            .iter()
            .map(|member| member.superblock.data_size_sectors)
            .min()
            .unwrap_or_default();
        plan.capacity_mismatch = plan
            .members
            .iter()
            .any(|member| member.superblock.data_size_sectors != plan.min_data_size_sectors);
    }
    Ok(plans)
}

pub fn open_mdraid0_reader(
    plan: &MdRaidArrayPlan,
) -> Result<fs_lvm::mdraid::MdRaid0Reader, MdRaidPlanError> {
    let mut members = Vec::with_capacity(plan.members.len());
    for member in &plan.members {
        let readers = member
            .lvm_identity
            .pv_sources
            .iter()
            .map(|source| {
                let kind = source.source_kind.as_ref().ok_or_else(|| {
                    MdRaidPlanError::Classification(datasource_service::DataSourceError::Evidence(
                        "LVM PV source kind is missing".to_string(),
                    ))
                })?;
                datasource_service::open_evidence_reader(
                    std::path::Path::new(&source.source_path),
                    kind,
                )
                .map_err(MdRaidPlanError::Io)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let offsets = member.lvm_identity.pv_offsets.clone();
        let pool = fs_lvm::LvmPool::discover(readers, offsets)?;
        let volumes = pool.list_volumes();
        let index = volumes
            .iter()
            .position(|volume| {
                (!member.lv_uuid.is_empty() && volume.uuid == member.lv_uuid)
                    || (member.lv_uuid.is_empty() && volume.name == member.lv_name)
            })
            .ok_or_else(|| {
                MdRaidPlanError::Classification(datasource_service::DataSourceError::Evidence(
                    format!("LVM logical volume '{}' was not found", member.lv_name),
                ))
            })?;
        let reader = pool.open_volume_reader(index)?;
        members.push(fs_lvm::mdraid::MdRaidMember::new(
            member.superblock.clone(),
            reader,
        ));
    }
    Ok(fs_lvm::mdraid::MdRaid0Reader::open(members)?)
}

fn inspect_pv(
    reader: &mut Box<dyn evidence_core::EvidenceReader>,
    source_path: &std::path::Path,
    source_kind: &DataSourceKind,
    offset: u64,
) -> Result<Option<PvEvidence>, MdRaidPlanError> {
    let label = fs_lvm::label::parse_pv_label(reader, offset)?;
    let mut best = None;
    for area in &label.metadata_areas {
        if let Ok(volume_group) = fs_lvm::metadata::parse_metadata(reader, area, offset) {
            let newer = best
                .as_ref()
                .is_none_or(|current: &fs_lvm::VolumeGroup| volume_group.seqno > current.seqno);
            if newer {
                best = Some(volume_group);
            }
        }
    }
    let Some(volume_group) = best else {
        return Ok(None);
    };
    Ok(Some(PvEvidence {
        source: LvmPhysicalVolumeSource {
            source_path: source_path.to_string_lossy().into_owned(),
            source_kind: Some(source_kind.clone()),
            offset,
            pv_uuid: label.pv_uuid.clone(),
            pv_name: None,
        },
        label,
        volume_group,
    }))
}

fn normalized_vg_key(volume_group: &fs_lvm::VolumeGroup) -> String {
    let id = volume_group
        .id
        .chars()
        .filter(|character| *character != '-')
        .collect::<String>()
        .to_ascii_lowercase();
    if id.is_empty() {
        format!("name:{}", volume_group.name)
    } else {
        format!("id:{id}")
    }
}

fn open_complete_pool(evidence: &[PvEvidence]) -> Result<Option<fs_lvm::LvmPool>, MdRaidPlanError> {
    let Some(volume_group) = evidence.first().map(|item| &item.volume_group) else {
        return Ok(None);
    };
    let mut readers = Vec::new();
    let mut offsets = Vec::new();
    for pv in &volume_group.physical_volumes {
        let Some(found) = evidence
            .iter()
            .find(|item| same_uuid(&item.label.pv_uuid, &pv.uuid))
        else {
            return Ok(None);
        };
        readers.push(datasource_service::open_evidence_reader(
            std::path::Path::new(&found.source.source_path),
            found
                .source
                .source_kind
                .as_ref()
                .unwrap_or(&DataSourceKind::E01),
        )?);
        offsets.push(found.source.offset);
    }
    Ok(Some(fs_lvm::LvmPool::discover(readers, offsets)?))
}

fn pool_sources(evidence: &[PvEvidence], pool: &fs_lvm::LvmPool) -> Vec<LvmPhysicalVolumeSource> {
    pool.physical_volume_offsets()
        .iter()
        .filter_map(|(name, offset)| {
            evidence
                .iter()
                .find(|item| item.source.offset == *offset)
                .map(|item| {
                    let mut source = item.source.clone();
                    source.pv_name = Some(name.clone());
                    source
                })
        })
        .collect()
}

fn insert_member(arrays: &mut BTreeMap<[u8; 16], MdRaidArrayPlan>, member: MdRaidMemberPlan) {
    let key = member.superblock.array_uuid;
    let plan = arrays.entry(key).or_insert_with(|| MdRaidArrayPlan {
        array_uuid: key,
        name: member.superblock.name.clone(),
        level: member.superblock.level,
        layout: member.superblock.layout,
        chunk_sectors: member.superblock.chunk_sectors,
        raid_disks: member.superblock.raid_disks,
        min_data_size_sectors: member.superblock.data_size_sectors,
        capacity_mismatch: false,
        members: Vec::new(),
    });
    if !plan
        .members
        .iter()
        .any(|known| known.superblock.dev_number == member.superblock.dev_number)
    {
        plan.members.push(member);
    }
}

fn same_uuid(left: &str, right: &str) -> bool {
    left.to_ascii_lowercase()
        .chars()
        .filter(|character| *character != '-')
        .eq(right
            .to_ascii_lowercase()
            .chars()
            .filter(|character| *character != '-'))
}
