use super::{MdRaidArrayPlan, MdRaidMemberOrigin, MdRaidMemberPlan, MdRaidPlanError};
use crate::datasource_service::{self, LvmPhysicalVolumeSource};
use domain::DataSourceKind;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::io::SeekFrom;
use std::sync::{Mutex, OnceLock};

use super::super::linux_import::LinuxEvidenceSetImportPlan;

type Cache = BTreeMap<String, CacheEntry>;

#[derive(Clone)]
struct CacheEntry {
    fingerprint: u64,
    arrays: Vec<MdRaidArrayPlan>,
}

struct PvEvidence {
    source: LvmPhysicalVolumeSource,
    label: fs_lvm::LvmLabel,
    volume_group: fs_lvm::VolumeGroup,
}

static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();

pub fn discover_linux_evidence_set_mdraid(
    plan: &LinuxEvidenceSetImportPlan,
) -> Result<Vec<MdRaidArrayPlan>, MdRaidPlanError> {
    let cache_key = plan.root_path.to_string_lossy().to_string();
    let fingerprint = plan_fingerprint(plan);
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    {
        let guard = cache.lock().map_err(|_| {
            MdRaidPlanError::Io(std::io::Error::other("MD RAID cache lock poisoned"))
        })?;
        if let Some(entry) = guard.get(&cache_key) {
            if entry.fingerprint == fingerprint {
                return Ok(entry.arrays.clone());
            }
        }
    }
    let arrays = discover_uncached(plan)?;
    let mut guard = cache
        .lock()
        .map_err(|_| MdRaidPlanError::Io(std::io::Error::other("MD RAID cache lock poisoned")))?;
    if guard.len() >= 8 {
        if let Some(first) = guard.keys().next().cloned() {
            guard.remove(&first);
        }
    }
    guard.insert(
        cache_key,
        CacheEntry {
            fingerprint,
            arrays: arrays.clone(),
        },
    );
    Ok(arrays)
}

fn discover_uncached(
    plan: &LinuxEvidenceSetImportPlan,
) -> Result<Vec<MdRaidArrayPlan>, MdRaidPlanError> {
    let mut arrays = BTreeMap::new();
    for member in &plan.members {
        let mut reader =
            datasource_service::open_evidence_reader(&member.source_path, &member.source_kind)?;
        let probe = datasource_service::detect_image_filesystem(&mut reader)?;
        discover_direct_member(
            &member.source_path,
            &member.source_kind,
            &probe,
            &mut arrays,
        )?;
    }
    merge_lvm_arrays(plan, &mut arrays)?;
    let mut plans = arrays.into_values().collect::<Vec<_>>();
    plans.retain(|plan| plan.members.len() <= plan.raid_disks as usize);
    for array in &mut plans {
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
    Ok(plans)
}

fn merge_lvm_arrays(
    plan: &LinuxEvidenceSetImportPlan,
    arrays: &mut BTreeMap<[u8; 16], MdRaidArrayPlan>,
) -> Result<(), MdRaidPlanError> {
    let pvs = collect_pvs(plan)?;
    let mut groups = BTreeMap::<String, Vec<PvEvidence>>::new();
    for pv in pvs {
        groups
            .entry(normalized_vg_key(&pv.volume_group))
            .or_default()
            .push(pv);
    }
    for evidence in groups.into_values() {
        let Some(pool) = open_complete_pool(&evidence)? else {
            continue;
        };
        let sources = pool_sources(&evidence, &pool);
        for (index, volume) in pool.list_volumes().iter().enumerate() {
            let mut reader = pool.open_volume_reader(index)?;
            let size = evidence_core::EvidenceReader::info(&*reader).size;
            let Some(superblock) =
                fs_lvm::mdraid::probe_superblock(&mut *reader, size).map(|probe| probe.superblock)
            else {
                log_probe(&mut *reader, size, &volume.name);
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

fn collect_pvs(plan: &LinuxEvidenceSetImportPlan) -> Result<Vec<PvEvidence>, MdRaidPlanError> {
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
    Ok(pvs)
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
    Ok(best.map(|volume_group| PvEvidence {
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
        members: Vec::new(),
    });
    if let Some(index) = plan
        .members
        .iter()
        .position(|known| known.superblock.slot_index() == Some(slot))
    {
        let known = &plan.members[index];
        if known.superblock.device_uuid == member.superblock.device_uuid {
            if member.superblock.events > known.superblock.events {
                plan.members[index] = member;
            }
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

fn plan_fingerprint(plan: &LinuxEvidenceSetImportPlan) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for member in &plan.members {
        member.source_path.hash(&mut hasher);
        member.source_kind.to_string().hash(&mut hasher);
        if let Ok(metadata) = std::fs::metadata(&member.source_path) {
            metadata.len().hash(&mut hasher);
            metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
                .hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn log_probe(reader: &mut dyn evidence_core::EvidenceReader, size: u64, volume_name: &str) {
    let sectors = size / 512;
    for (offset, version) in [
        (4096u64, fs_lvm::mdraid::MdRaidMetadataVersion::V1_2),
        (0u64, fs_lvm::mdraid::MdRaidMetadataVersion::V1_1),
        (
            (sectors.saturating_sub(16) & !7) * 512,
            fs_lvm::mdraid::MdRaidMetadataVersion::V1_0,
        ),
    ] {
        let mut bytes = [0u8; 4096];
        let parsed = reader
            .seek(SeekFrom::Start(offset))
            .and_then(|_| reader.read_exact(&mut bytes))
            .map_err(|error| error.to_string())
            .and_then(|_| {
                fs_lvm::mdraid::parse_superblock_version(&bytes, version)
                    .map_err(|error| error.to_string())
            });
        tracing::debug!(
            volume = volume_name,
            offset,
            ?version,
            result = ?parsed.map(|superblock| (
                superblock.name,
                superblock.level,
                superblock.dev_number,
                superblock.events,
                superblock.super_offset_sectors,
                superblock.checksum_valid,
            )),
            "MD RAID superblock probe candidate"
        );
    }
}
