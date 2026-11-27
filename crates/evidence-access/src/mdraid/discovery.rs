use super::{MdRaidArrayPlan, MdRaidMemberOrigin, MdRaidMemberPlan, MdRaidPlanError};
use crate::datasource_service::{self, LvmPhysicalVolumeSource};
use crate::source_runtime::snapshot::SourceSnapshot;
use domain::DataSourceKind;
use std::collections::BTreeMap;

/// One physical member of a Linux evidence set, as needed for MD RAID discovery.
pub struct MdRaidDiscoveryMember<'a> {
    pub source_path: &'a std::path::Path,
    pub source_kind: &'a DataSourceKind,
}

#[derive(Clone)]
struct PvEvidence {
    source: LvmPhysicalVolumeSource,
    label: fs_lvm::LvmLabel,
    volume_group: Option<fs_lvm::VolumeGroup>,
}

pub fn discover_linux_evidence_set_mdraid(
    members: &[MdRaidDiscoveryMember<'_>],
) -> Result<Vec<MdRaidArrayPlan>, MdRaidPlanError> {
    discover_linux_evidence_set_mdraid_with_cancel(
        members,
        &std::sync::atomic::AtomicBool::new(false),
    )
}

pub fn discover_linux_evidence_set_mdraid_with_cancel(
    members: &[MdRaidDiscoveryMember<'_>],
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Vec<MdRaidArrayPlan>, MdRaidPlanError> {
    let mut arrays = BTreeMap::new();
    let mut pvs = Vec::new();
    for member in members {
        if cancel.load(std::sync::atomic::Ordering::Acquire) {
            return Err(MdRaidPlanError::Cancelled);
        }
        let mut reader =
            datasource_service::open_evidence_reader(member.source_path, member.source_kind)?;
        let probe = datasource_service::detect_image_filesystem(&mut reader)?;
        discover_direct_member(member.source_path, member.source_kind, &probe, &mut arrays)?;
        collect_pvs(member, &probe, &mut reader, &mut pvs)?;
    }
    merge_lvm_arrays(pvs, &mut arrays)?;
    let mut plans = arrays.into_values().collect::<Vec<_>>();
    for array in &mut plans {
        array.source_snapshot = capture_array_sources(array)?;
        array
            .members
            .sort_by_key(|member| member.superblock.slot_index());
        array.min_data_size_sectors = array
            .members
            .iter()
            .map(|member| member.superblock.data_size_sectors)
            .min()
            .unwrap_or_default();
        array.capacity_mismatch = array
            .members
            .iter()
            .any(|member| member.superblock.data_size_sectors != array.min_data_size_sectors);
    }
    super::content::bind_content(&mut plans, cancel)?;
    Ok(plans)
}

pub(super) fn capture_array_sources(
    plan: &MdRaidArrayPlan,
) -> Result<SourceSnapshot, MdRaidPlanError> {
    let mut paths = BTreeMap::new();
    for member in &plan.members {
        match &member.origin {
            MdRaidMemberOrigin::Direct {
                source_path,
                source_kind,
                ..
            } => {
                paths.insert(source_path.clone(), source_kind.clone());
            }
            MdRaidMemberOrigin::Lvm { identity, .. } => {
                for source in &identity.pv_sources {
                    let kind = source.source_kind.clone().ok_or_else(|| {
                        std::io::Error::other("LVM physical volume source kind is missing")
                    })?;
                    paths.insert(source.source_path.clone(), kind);
                }
            }
        }
    }
    let mut backing_paths = std::collections::BTreeSet::new();
    for (source_path, kind) in paths {
        let path = std::path::Path::new(&source_path);
        match kind {
            DataSourceKind::E01 => backing_paths.extend(
                image_e01::E01Reader::open(path)?
                    .open_plan()?
                    .segment_paths(),
            ),
            DataSourceKind::Raw => backing_paths.extend(
                evidence_core::RawImageReader::open(path)?
                    .backing_paths()
                    .iter()
                    .cloned(),
            ),
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "MD RAID requires immutable RAW or E01 sources",
                )
                .into())
            }
        }
    }
    Ok(SourceSnapshot::capture(
        &backing_paths.into_iter().collect::<Vec<_>>(),
    )?)
}

fn merge_lvm_arrays(
    pvs: Vec<PvEvidence>,
    arrays: &mut BTreeMap<[u8; 16], MdRaidArrayPlan>,
) -> Result<(), MdRaidPlanError> {
    let mut groups = BTreeMap::<String, Vec<PvEvidence>>::new();
    let mut without_metadata = Vec::new();
    for pv in pvs {
        let Some(volume_group) = &pv.volume_group else {
            without_metadata.push(pv);
            continue;
        };
        groups
            .entry(normalized_vg_key(volume_group))
            .or_default()
            .push(pv);
    }
    for mut evidence in groups.into_values() {
        let volume_group = fs_lvm::metadata::select_committed_metadata(
            evidence.iter().filter_map(|pv| pv.volume_group.as_ref()),
        )?
        .ok_or_else(|| std::io::Error::other("committed LVM metadata is missing"))?;
        let members = without_metadata
            .iter()
            .filter(|pv| {
                volume_group
                    .physical_volumes
                    .iter()
                    .any(|member| same_uuid(&pv.label.pv_uuid, &member.uuid))
            })
            .cloned()
            .collect::<Vec<_>>();
        evidence.extend(members);
        let Some(pool) = open_complete_pool(&evidence)? else {
            continue;
        };
        let sources = pool_sources(&evidence, &pool)?;
        for (index, volume) in pool.list_readable_volumes() {
            let mut reader = pool.open_volume_reader(index)?;
            let size = evidence_core::EvidenceReader::info(&*reader).size;
            let Some(superblock) =
                fs_lvm::mdraid::probe_superblock(&mut *reader, size).map(|probe| probe.superblock)
            else {
                continue;
            };
            let identity = datasource_service::LvmLogicalVolumeIdentity {
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
            insert_member(
                arrays,
                MdRaidMemberPlan {
                    superblock,
                    origin: MdRaidMemberOrigin::Lvm {
                        vg_name: pool.volume_group().name.clone(),
                        vg_uuid: pool.volume_group().id.clone(),
                        lv_name: volume.name.clone(),
                        lv_uuid: volume.uuid.clone(),
                        identity,
                    },
                },
            )?;
        }
    }
    Ok(())
}

fn collect_pvs(
    member: &MdRaidDiscoveryMember<'_>,
    probe: &datasource_service::ImageFilesystemProbe,
    reader: &mut Box<dyn evidence_core::EvidenceReader>,
    pvs: &mut Vec<PvEvidence>,
) -> Result<(), MdRaidPlanError> {
    for candidate in probe
        .candidates
        .iter()
        .filter(|candidate| candidate.kind == datasource_service::ImageFilesystemKind::LvmPool)
    {
        let evidence = inspect_pv(
            reader,
            member.source_path,
            member.source_kind,
            candidate.offset,
        )?;
        if !pvs.iter().any(|known: &PvEvidence| {
            known.source.offset == evidence.source.offset
                && known.source.source_path == evidence.source.source_path
        }) {
            pvs.push(evidence);
        }
    }
    Ok(())
}

fn discover_direct_member(
    source_path: &std::path::Path,
    source_kind: &DataSourceKind,
    probe: &datasource_service::ImageFilesystemProbe,
    arrays: &mut BTreeMap<[u8; 16], MdRaidArrayPlan>,
) -> Result<(), MdRaidPlanError> {
    for partition in &probe.partitions {
        if partition.length == 0 {
            continue;
        }
        if let Some((superblock, byte_length)) = probe_window(
            source_path,
            source_kind,
            partition.offset,
            Some(partition.length),
        )? {
            insert_member(
                arrays,
                MdRaidMemberPlan {
                    superblock,
                    origin: MdRaidMemberOrigin::Direct {
                        source_path: source_path.to_string_lossy().into_owned(),
                        source_kind: source_kind.clone(),
                        byte_offset: partition.offset,
                        byte_length,
                    },
                },
            )?;
        }
    }
    if let Some((superblock, byte_length)) = probe_window(source_path, source_kind, 0, None)? {
        insert_member(
            arrays,
            MdRaidMemberPlan {
                superblock,
                origin: MdRaidMemberOrigin::Direct {
                    source_path: source_path.to_string_lossy().into_owned(),
                    source_kind: source_kind.clone(),
                    byte_offset: 0,
                    byte_length,
                },
            },
        )?;
    }
    Ok(())
}

fn probe_window(
    source_path: &std::path::Path,
    source_kind: &DataSourceKind,
    offset: u64,
    length: Option<u64>,
) -> Result<Option<(fs_lvm::mdraid::MdRaidSuperblock, u64)>, MdRaidPlanError> {
    let reader = datasource_service::open_evidence_reader(source_path, source_kind)?;
    let mut window = evidence_core::PartitionWindowReader::new(reader, offset, length)?;
    let size = evidence_core::EvidenceReader::info(&window).size;
    Ok(fs_lvm::mdraid::probe_superblock(&mut window, size).map(|probe| (probe.superblock, size)))
}

fn inspect_pv(
    reader: &mut Box<dyn evidence_core::EvidenceReader>,
    source_path: &std::path::Path,
    source_kind: &DataSourceKind,
    offset: u64,
) -> Result<PvEvidence, MdRaidPlanError> {
    let label = fs_lvm::label::parse_pv_label(reader, offset)?;
    let volume_group = if label.metadata_areas.is_empty() {
        None
    } else {
        Some(fs_lvm::metadata::parse_metadata_from_regions(
            reader,
            &label.metadata_areas,
            offset,
        )?)
    };
    Ok(PvEvidence {
        source: LvmPhysicalVolumeSource {
            source_path: source_path.to_string_lossy().into_owned(),
            source_kind: Some(source_kind.clone()),
            offset,
            pv_uuid: label.pv_uuid.clone(),
            pv_name: None,
        },
        label,
        volume_group,
    })
}

fn open_complete_pool(evidence: &[PvEvidence]) -> Result<Option<fs_lvm::LvmPool>, MdRaidPlanError> {
    let Some(volume_group) = fs_lvm::metadata::select_committed_metadata(
        evidence
            .iter()
            .filter_map(|item| item.volume_group.as_ref()),
    )?
    else {
        return Ok(None);
    };
    let mut readers = Vec::new();
    let mut offsets = Vec::new();
    for pv in &volume_group.physical_volumes {
        let matching = evidence
            .iter()
            .filter(|item| same_uuid(&item.label.pv_uuid, &pv.uuid))
            .collect::<Vec<_>>();
        let Some(found) = matching.first() else {
            return Ok(None);
        };
        if matching.len() != 1 {
            return Err(fs_lvm::LvmError::MetadataParseError {
                line: 0,
                message: format!("multiple sources claim physical volume {}", pv.name),
            }
            .into());
        }
        readers.push(datasource_service::open_evidence_reader(
            std::path::Path::new(&found.source.source_path),
            found
                .source
                .source_kind
                .as_ref()
                .ok_or_else(|| std::io::Error::other("physical volume source kind is missing"))?,
        )?);
        offsets.push(found.source.offset);
    }
    Ok(Some(fs_lvm::LvmPool::discover(readers, offsets)?))
}

fn pool_sources(
    evidence: &[PvEvidence],
    pool: &fs_lvm::LvmPool,
) -> Result<Vec<LvmPhysicalVolumeSource>, MdRaidPlanError> {
    pool.physical_volume_offsets()
        .iter()
        .map(|(name, offset)| {
            let pv = pool
                .volume_group()
                .physical_volumes
                .iter()
                .find(|pv| pv.name == *name)
                .ok_or_else(|| std::io::Error::other("physical volume metadata is missing"))?;
            let item = evidence
                .iter()
                .find(|item| {
                    item.source.offset == *offset && same_uuid(&item.label.pv_uuid, &pv.uuid)
                })
                .ok_or_else(|| {
                    std::io::Error::other("physical volume source binding is missing")
                })?;
            let mut source = item.source.clone();
            source.pv_name = Some(name.clone());
            Ok(source)
        })
        .collect()
}

fn insert_member(
    arrays: &mut BTreeMap<[u8; 16], MdRaidArrayPlan>,
    member: MdRaidMemberPlan,
) -> Result<(), MdRaidPlanError> {
    let slot = member.superblock.slot_index().ok_or_else(|| {
        MdRaidPlanError::MdRaid(fs_lvm::mdraid::MdRaidError::MetadataConflict(format!(
            "device {} has no active role",
            member.superblock.dev_number
        )))
    })?;
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
        source_snapshot: SourceSnapshot::default(),
        source_content: Vec::new(),
        members: Vec::new(),
    });
    if let Some(index) = plan
        .members
        .iter()
        .position(|known| known.superblock.slot_index() == Some(slot))
    {
        let known = &plan.members[index];
        if known == &member {
            return Ok(());
        }
        return Err(MdRaidPlanError::MdRaid(
            fs_lvm::mdraid::MdRaidError::MetadataConflict(format!(
                "multiple devices claim RAID slot {slot}"
            )),
        ));
    }
    plan.members.push(member);
    Ok(())
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

fn same_uuid(left: &str, right: &str) -> bool {
    left.to_ascii_lowercase()
        .chars()
        .filter(|character| *character != '-')
        .eq(right
            .to_ascii_lowercase()
            .chars()
            .filter(|character| *character != '-'))
}

#[cfg(test)]
#[path = "../../tests/unit/mdraid_discovery.rs"]
mod tests;
