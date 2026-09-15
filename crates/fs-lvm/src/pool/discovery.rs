use super::mapping::{resolve_pv_mapping, validate_extent_map};
use super::{DiscoveredPv, LvmPool, SharedReader};
use crate::error::{LvmError, Result};
use crate::metadata::VolumeGroup;
use crate::{label, metadata, segment};
use evidence_core::EvidenceReader;
use std::sync::{Arc, Mutex, MutexGuard};

impl LvmPool {
    pub fn discover(readers: Vec<Box<dyn EvidenceReader>>, pv_offsets: Vec<u64>) -> Result<Self> {
        if readers.is_empty() || readers.len() != pv_offsets.len() {
            return Err(LvmError::MetadataParseError {
                line: 0,
                message: "readers and pv_offsets must be non-empty and same length".to_string(),
            });
        }

        let pv_entries = discover_physical_volumes(readers, pv_offsets)?;
        let (volume_group, warnings) = select_volume_group(&pv_entries)?;
        let mut device_readers = Vec::with_capacity(volume_group.physical_volumes.len());
        let mut mappings = Vec::with_capacity(volume_group.physical_volumes.len());

        for pv_meta in &volume_group.physical_volumes {
            let matched = pv_entries
                .iter()
                .find(|entry| lvm_uuid_matches(&entry.label.pv_uuid, &pv_meta.uuid))
                .ok_or_else(|| LvmError::MissingPhysicalVolumeReader {
                    pv_name: pv_meta.name.clone(),
                    pv_uuid: pv_meta.uuid.clone(),
                })?;
            mappings.push(resolve_pv_mapping(pv_meta, matched)?);
            device_readers.push(matched.reader.clone());
        }

        let logical_volumes = volume_group.logical_volumes.clone();
        let pv_start_offsets = mappings
            .iter()
            .map(|mapping| (mapping.name.clone(), mapping.start_offset))
            .collect::<Vec<_>>();
        let pv_data_offsets = mappings
            .iter()
            .map(|mapping| (mapping.name.clone(), mapping.data_offset))
            .collect::<Vec<_>>();
        let pv_bounds = mappings
            .iter()
            .map(|mapping| {
                (
                    mapping.name.clone(),
                    mapping.data_offset,
                    mapping.data_size,
                    mapping.pv_size,
                )
            })
            .collect::<Vec<_>>();

        for logical_volume in &logical_volumes {
            if logical_volume.is_directly_mappable() {
                let extent_map =
                    segment::build_extent_map(&volume_group, logical_volume, &pv_data_offsets)?;
                validate_extent_map(logical_volume, &extent_map, &pv_bounds)?;
            }
        }

        Ok(Self {
            volume_group,
            device_readers,
            pv_start_offsets,
            pv_data_offsets,
            logical_volumes,
            warnings,
        })
    }
}

fn discover_physical_volumes(
    readers: Vec<Box<dyn EvidenceReader>>,
    pv_offsets: Vec<u64>,
) -> Result<Vec<DiscoveredPv>> {
    readers
        .into_iter()
        .zip(pv_offsets)
        .map(|(reader, pv_offset)| {
            let reader = Arc::new(Mutex::new(reader));
            let label = {
                let mut guard = lock_pv_reader(&reader)?;
                label::parse_pv_label(&mut **guard, pv_offset)?
            };
            Ok(DiscoveredPv {
                reader,
                label,
                pv_offset,
            })
        })
        .collect()
}

fn select_volume_group(entries: &[DiscoveredPv]) -> Result<(VolumeGroup, Vec<String>)> {
    let mut selected: Option<VolumeGroup> = None;
    let mut candidates: Vec<VolumeGroupIdentity> = Vec::new();
    let mut first_fatal_error = None;
    for entry in entries {
        if entry.label.metadata_areas.is_empty() {
            continue;
        }
        let mut reader = lock_pv_reader(&entry.reader)?;
        let candidate = match metadata::parse_metadata_from_regions(
            &mut *reader,
            &entry.label.metadata_areas,
            entry.pv_offset,
        ) {
            Ok(candidate) => candidate,
            Err(LvmError::MetadataParseError { .. })
            | Err(LvmError::MdaCrcMismatch { .. })
            | Err(LvmError::MetadataCrcMismatch { .. }) => continue,
            Err(error @ LvmError::FatalMetadataParseError { .. }) => {
                if first_fatal_error.is_none() {
                    first_fatal_error = Some(error);
                }
                continue;
            }
            Err(error) => return Err(error),
        };
        remember_identity(&mut candidates, &candidate);
        if selected
            .as_ref()
            .is_none_or(|current| candidate.seqno > current.seqno)
        {
            selected = Some(candidate);
        }
    }
    match selected {
        Some(volume_group) => {
            let warnings = multi_vg_warnings(&candidates, &volume_group);
            Ok((volume_group, warnings))
        }
        None => match first_fatal_error {
            Some(error) => Err(error),
            None => Err(LvmError::MetadataParseError {
                line: 0,
                message: "no valid metadata copy found on supplied physical volumes".to_string(),
            }),
        },
    }
}

struct VolumeGroupIdentity {
    name: String,
    id: String,
    seqno: u64,
}

fn remember_identity(candidates: &mut Vec<VolumeGroupIdentity>, volume_group: &VolumeGroup) {
    if let Some(known) = candidates
        .iter_mut()
        .find(|known| known.name == volume_group.name && known.id == volume_group.id)
    {
        known.seqno = known.seqno.max(volume_group.seqno);
        return;
    }
    candidates.push(VolumeGroupIdentity {
        name: volume_group.name.clone(),
        id: volume_group.id.clone(),
        seqno: volume_group.seqno,
    });
}

fn multi_vg_warnings(candidates: &[VolumeGroupIdentity], selected: &VolumeGroup) -> Vec<String> {
    if candidates.len() <= 1 {
        return Vec::new();
    }
    let ignored = candidates
        .iter()
        .filter(|candidate| candidate.name != selected.name || candidate.id != selected.id)
        .map(|candidate| {
            format!(
                "'{}' (id={}, seqno={})",
                candidate.name, candidate.id, candidate.seqno
            )
        })
        .collect::<Vec<_>>();
    vec![format!(
        "multiple volume groups found on supplied physical volumes; selected '{}' (id={}, seqno={}); ignored: {}",
        selected.name,
        selected.id,
        selected.seqno,
        ignored.join(", ")
    )]
}

fn lock_pv_reader(reader: &SharedReader) -> Result<MutexGuard<'_, Box<dyn EvidenceReader>>> {
    reader.lock().map_err(|_| LvmError::MetadataParseError {
        line: 0,
        message: "physical volume reader lock poisoned".to_string(),
    })
}

fn lvm_uuid_matches(label_uuid: &str, metadata_uuid: &str) -> bool {
    let normalize = |uuid: &str| {
        uuid.trim()
            .chars()
            .filter(|character| *character != '-')
            .collect::<String>()
            .to_ascii_lowercase()
    };
    let label = normalize(label_uuid);
    let metadata = normalize(metadata_uuid);
    !label.is_empty() && label == metadata
}
