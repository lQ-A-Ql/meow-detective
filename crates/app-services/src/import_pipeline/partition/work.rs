use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;

use crate::datasource_service::{
    self, ImageFilesystemKind, ImageFilesystemSource, LvmLogicalVolumeIdentity,
    LvmPhysicalVolumeSource,
};
use crate::parallel_enum;

pub fn build_partition_work(
    source_path: &std::path::Path,
    source_kind: &domain::DataSourceKind,
    partition_index: usize,
    partition_name: &str,
    fs_kind: &str,
    probe_candidates: &[datasource_service::ImageFilesystemCandidate],
) -> Option<parallel_enum::PartitionWork> {
    let index_map = datasource_service::assign_effective_partition_indices(probe_candidates);
    let candidate = probe_candidates
        .iter()
        .enumerate()
        .find(|(ordinal, candidate)| {
            datasource_service::effective_partition_index(candidate, *ordinal, &index_map)
                == partition_index
        })
        .map(|(_, candidate)| candidate)?;
    let fs = open_candidate_filesystem(source_path, source_kind, candidate).ok()??;
    Some(parallel_enum::PartitionWork {
        index: partition_index,
        name: partition_name.to_string(),
        fs_kind: fs_kind.to_string(),
        fs,
        source_path: source_path.to_path_buf(),
        source_kind: format!("{source_kind:?}"),
        volume_offset: candidate.offset,
    })
}

pub(super) fn open_candidate_filesystem(
    source_path: &std::path::Path,
    source_kind: &domain::DataSourceKind,
    candidate: &datasource_service::ImageFilesystemCandidate,
) -> Result<Option<Box<dyn FileSystemReader + Send>>, String> {
    let (base_reader, fs_offset) = open_candidate_reader(source_path, source_kind, candidate)?;
    let fs: Box<dyn FileSystemReader + Send> = match candidate.kind {
        ImageFilesystemKind::Ntfs => Box::new(
            fs_ntfs::NtfsReader::open(base_reader, fs_offset).map_err(|error| error.to_string())?,
        ),
        ImageFilesystemKind::Fat => Box::new(
            fs_fat::FatReader::open(base_reader, fs_offset).map_err(|error| error.to_string())?,
        ),
        ImageFilesystemKind::Ext4 => Box::new(
            fs_ext4::Ext4Reader::open(base_reader, fs_offset).map_err(|error| error.to_string())?,
        ),
        ImageFilesystemKind::Xfs => Box::new(
            fs_xfs::XfsReader::open(base_reader, fs_offset).map_err(|error| error.to_string())?,
        ),
        ImageFilesystemKind::Btrfs => Box::new(
            fs_btrfs::BtrfsReader::open(base_reader, fs_offset)
                .map_err(|error| error.to_string())?,
        ),
        ImageFilesystemKind::LvmPool | ImageFilesystemKind::BitLocker => return Ok(None),
    };
    Ok(Some(fs))
}

pub(crate) fn open_candidate_reader(
    source_path: &std::path::Path,
    source_kind: &domain::DataSourceKind,
    candidate: &datasource_service::ImageFilesystemCandidate,
) -> Result<(Box<dyn EvidenceReader>, u64), String> {
    if matches!(candidate.source, ImageFilesystemSource::LvmLogicalVolume) {
        return open_lvm_candidate_reader(source_path, source_kind, candidate);
    }
    let reader: Box<dyn EvidenceReader> = match source_kind {
        domain::DataSourceKind::E01 => {
            Box::new(E01Reader::open(source_path).map_err(|error| error.to_string())?)
        }
        domain::DataSourceKind::Raw => Box::new(
            evidence_core::RawImageReader::open(source_path).map_err(|error| error.to_string())?,
        ),
        domain::DataSourceKind::LogicalDirectory => {
            return Err("logical directories do not expose image candidates".to_string())
        }
    };
    Ok((reader, candidate.offset))
}

fn open_lvm_candidate_reader(
    source_path: &std::path::Path,
    source_kind: &domain::DataSourceKind,
    candidate: &datasource_service::ImageFilesystemCandidate,
) -> Result<(Box<dyn EvidenceReader>, u64), String> {
    let identity = candidate
        .lvm_identity
        .as_ref()
        .ok_or_else(|| "LVM logical volume candidate missing identity".to_string())?;
    let readers = open_lvm_physical_volume_readers(source_path, source_kind, identity)
        .ok_or_else(|| "failed to open LVM physical volume readers".to_string())?;
    let pool = fs_lvm::LvmPool::discover(readers, identity.pv_offsets.clone())
        .map_err(|e| e.to_string())?;
    let volumes = pool.list_volumes();
    let volume_index = find_lvm_volume_index(&volumes, identity)
        .ok_or_else(|| "LVM logical volume identity not found in pool".to_string())?;
    let reader = pool
        .open_volume_reader(volume_index)
        .map_err(|error| error.to_string())?;
    Ok((reader, 0))
}

fn open_lvm_physical_volume_readers(
    source_path: &std::path::Path,
    source_kind: &domain::DataSourceKind,
    identity: &LvmLogicalVolumeIdentity,
) -> Option<Vec<Box<dyn EvidenceReader>>> {
    if identity.pv_offsets.is_empty() {
        return None;
    }
    if identity.pv_offsets.len() > 1 && identity.pv_sources.len() != identity.pv_offsets.len() {
        tracing::warn!(
            pv_offsets = identity.pv_offsets.len(),
            pv_sources = identity.pv_sources.len(),
            vg = %identity.vg_name,
            lv = %identity.lv_name,
            "LVM identity is missing per-PV source paths; refusing multi-PV fallback"
        );
        return None;
    }

    identity
        .pv_offsets
        .iter()
        .enumerate()
        .map(|(index, offset)| {
            open_lvm_physical_volume_reader(source_path, source_kind, identity, index, *offset)
        })
        .collect()
}

fn open_lvm_physical_volume_reader(
    default_path: &std::path::Path,
    default_kind: &domain::DataSourceKind,
    identity: &LvmLogicalVolumeIdentity,
    index: usize,
    offset: u64,
) -> Option<Box<dyn EvidenceReader>> {
    let source = identity.pv_sources.get(index);
    let path = source
        .map(|value| std::path::Path::new(&value.source_path))
        .unwrap_or(default_path);
    let kind = source
        .and_then(|value| value.source_kind.as_ref())
        .unwrap_or(default_kind);
    let mut reader: Box<dyn EvidenceReader> = match kind {
        domain::DataSourceKind::E01 => Box::new(E01Reader::open(path).ok()?),
        domain::DataSourceKind::Raw => Box::new(evidence_core::RawImageReader::open(path).ok()?),
        domain::DataSourceKind::LogicalDirectory => return None,
    };
    if let Some(expected) = source {
        validate_lvm_pv_source(reader.as_mut(), offset, expected)?;
    }
    Some(reader)
}

fn validate_lvm_pv_source(
    reader: &mut dyn EvidenceReader,
    expected_offset: u64,
    expected_source: &LvmPhysicalVolumeSource,
) -> Option<()> {
    if expected_source.pv_uuid.is_empty() {
        return Some(());
    }
    let label = match fs_lvm::label::parse_pv_label(reader, expected_offset) {
        Ok(label) => label,
        Err(error) => {
            tracing::warn!(
                source = %datasource_service::lvm_source_fingerprint(&expected_source.source_path),
                offset = expected_offset,
                error = %error,
                "LVM import PV source label validation failed"
            );
            return None;
        }
    };
    let actual = datasource_service::normalize_lvm_uuid_for_match(&label.pv_uuid);
    let expected = datasource_service::normalize_lvm_uuid_for_match(&expected_source.pv_uuid);
    if actual == expected {
        return Some(());
    }
    tracing::warn!(
        source = %datasource_service::lvm_source_fingerprint(&expected_source.source_path),
        offset = expected_offset,
        expected = %expected_source.pv_uuid,
        actual = %label.pv_uuid,
        "LVM import PV source UUID mismatch"
    );
    None
}

fn find_lvm_volume_index(
    volumes: &[fs_lvm::LvInfo],
    identity: &LvmLogicalVolumeIdentity,
) -> Option<usize> {
    if !identity.lv_uuid.is_empty() {
        return volumes
            .iter()
            .position(|volume| volume.uuid == identity.lv_uuid);
    }
    volumes
        .iter()
        .position(|volume| volume.name == identity.lv_name)
}

#[cfg(test)]
#[path = "../../../tests/unit/import_pipeline/partition/work.rs"]
mod tests;
