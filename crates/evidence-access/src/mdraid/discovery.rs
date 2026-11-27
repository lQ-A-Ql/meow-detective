use super::{MdRaidArrayPlan, MdRaidMemberOrigin, MdRaidMemberPlan, MdRaidPlanError};
use crate::{
    datasource_service::{ImageCollectionPlan, ImageFilesystemProbe, LvmDiscoverySource},
    source_runtime::snapshot::SourceSnapshot,
};
use domain::DataSourceKind;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub struct MdRaidDiscoveryMember<'a> {
    pub source_path: &'a Path,
    pub source_kind: &'a DataSourceKind,
}

pub fn discover_linux_evidence_set_mdraid(
    members: &[MdRaidDiscoveryMember<'_>],
) -> Result<Vec<MdRaidArrayPlan>, MdRaidPlanError> {
    discover_linux_evidence_set_mdraid_with_cancel(members, &AtomicBool::new(false))
}

pub fn discover_linux_evidence_set_mdraid_with_cancel(
    members: &[MdRaidDiscoveryMember<'_>],
    cancel: &AtomicBool,
) -> Result<Vec<MdRaidArrayPlan>, MdRaidPlanError> {
    let sources = members
        .iter()
        .map(|member| LvmDiscoverySource::new(member.source_path, member.source_kind.clone()))
        .collect::<Vec<_>>();
    let plan = ImageCollectionPlan::prepare(&sources, cancel)?;
    discover_mdraid_from_collection(&plan, cancel)
}

pub fn discover_mdraid_from_collection(
    collection: &ImageCollectionPlan,
    cancel: &AtomicBool,
) -> Result<Vec<MdRaidArrayPlan>, MdRaidPlanError> {
    if cancel.load(Ordering::Acquire) {
        return Err(MdRaidPlanError::Cancelled);
    }
    collection.validate_snapshot()?;
    let mut arrays = BTreeMap::new();
    for member in collection.md_members() {
        if cancel.load(Ordering::Acquire) {
            return Err(MdRaidPlanError::Cancelled);
        }
        insert_member(&mut arrays, member.clone())?;
    }
    let mut plans = arrays.into_values().collect::<Vec<_>>();
    for array in &mut plans {
        let sources = super::content::input_sources(array)?
            .into_iter()
            .map(|(path, kind)| LvmDiscoverySource::new(path, kind))
            .collect::<Vec<_>>();
        array.source_snapshot = collection.source_snapshot(&sources)?;
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
        array.source_content = super::content::input_sources(array)?
            .into_iter()
            .map(|(path, kind)| {
                collection
                    .source_content(Path::new(&path), &kind)
                    .map(|identity| (*identity).clone())
            })
            .collect::<Result<_, _>>()?;
    }
    collection.validate_snapshot()?;
    Ok(plans)
}

pub(crate) fn direct_members(
    source: &LvmDiscoverySource,
    probe: &ImageFilesystemProbe,
    factory: &evidence_core::SharedReaderFactory,
) -> Result<Vec<MdRaidMemberPlan>, crate::datasource_service::CollectionPlanError> {
    let mut ranges = probe
        .partitions
        .iter()
        .filter(|partition| partition.length != 0)
        .map(|partition| (partition.offset, Some(partition.length)))
        .collect::<Vec<_>>();
    ranges.push((0, None));
    ranges.sort_unstable();
    ranges.dedup();
    let mut members = Vec::new();
    for (offset, length) in ranges {
        let mut reader =
            evidence_core::PartitionWindowReader::new(factory.open_reader()?, offset, length)?;
        let size = evidence_core::EvidenceReader::info(&reader).size;
        if let Some(probe) = fs_lvm::mdraid::probe_superblock(&mut reader, size) {
            members.push(MdRaidMemberPlan {
                superblock: probe.superblock,
                origin: MdRaidMemberOrigin::Direct {
                    source_path: source.source_path.to_string_lossy().into_owned(),
                    source_kind: source.source_kind.clone(),
                    byte_offset: offset,
                    byte_length: size,
                },
            });
        }
    }
    Ok(members)
}

fn insert_member(
    arrays: &mut BTreeMap<[u8; 16], MdRaidArrayPlan>,
    member: MdRaidMemberPlan,
) -> Result<(), MdRaidPlanError> {
    let slot = member.superblock.slot_index().ok_or_else(|| {
        fs_lvm::mdraid::MdRaidError::MetadataConflict(format!(
            "device {} has no active role",
            member.superblock.dev_number
        ))
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
    if let Some(known) = plan
        .members
        .iter()
        .find(|known| known.superblock.slot_index() == Some(slot))
    {
        if known == &member {
            return Ok(());
        }
        return Err(fs_lvm::mdraid::MdRaidError::MetadataConflict(format!(
            "multiple devices claim RAID slot {slot}"
        ))
        .into());
    }
    plan.members.push(member);
    Ok(())
}
