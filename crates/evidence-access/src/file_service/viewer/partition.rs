//! Published partition identities used by source-scoped image previews.

use crate::file_service::{
    viewer::{PreviewDescriptor, PreviewPartitionCandidate},
    FileServiceError,
};
use crate::partition_query_service::PartitionQueryRepo;
use domain::FileEntry;
use rusqlite::Connection;

pub(crate) fn published_partition_candidates(
    conn: &Connection,
    entry: &FileEntry,
    expected_partition_index: Option<usize>,
) -> Result<Vec<PreviewPartitionCandidate>, FileServiceError> {
    let partitions = PartitionQueryRepo::new(conn)
        .find_by_data_source(&entry.data_source_id.0)
        .map_err(|error| FileServiceError::other(format!("Failed to query partitions: {error}")))?;
    if partitions.is_empty() {
        return Err(FileServiceError::other(
            "Published partition metadata is missing",
        ));
    }
    if expected_partition_index.is_none() && !entry.path.contains('/') && !entry.path.contains('\\')
    {
        return Err(FileServiceError::other(format!(
            "Cannot resolve partition identity for '{}'",
            entry.path
        )));
    }
    let candidates = partitions
        .iter()
        .filter(|partition| {
            expected_partition_index.is_none_or(|index| partition.partition_index as usize == index)
                && is_previewable_partition_status(&partition.status)
        })
        .map(preview_partition_candidate_from_record)
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err(FileServiceError::other(
            "Published readable partition identity is missing",
        ));
    }
    if candidates.len() != 1 {
        return Err(FileServiceError::other(format!(
            "Image preview requires an exact partition index; found {} candidates",
            candidates.len()
        )));
    }
    Ok(candidates)
}

pub(crate) fn is_previewable_partition_status(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "supported" | "queued" | "done" | "ready" | "encrypted_bitlocker"
    )
}

pub(crate) fn exact_partition_candidate(
    descriptor: &PreviewDescriptor,
) -> Result<&PreviewPartitionCandidate, FileServiceError> {
    let expected = descriptor.partition_index.ok_or_else(|| {
        FileServiceError::other(format!(
            "Image preview '{}' has no exact partition index",
            descriptor.path
        ))
    })?;
    let [candidate] = descriptor.partition_candidates.as_slice() else {
        return Err(FileServiceError::other(format!(
            "Image preview '{}' requires exactly one partition candidate, found {}",
            descriptor.path,
            descriptor.partition_candidates.len()
        )));
    };
    if candidate.partition_index != expected {
        return Err(FileServiceError::other(format!(
            "Image preview '{}' partition candidate {} does not match exact partition index {}",
            descriptor.path, candidate.partition_index, expected
        )));
    }
    if descriptor
        .filesystem_kind
        .as_deref()
        .is_some_and(|kind| !kind.eq_ignore_ascii_case(&candidate.filesystem_kind))
    {
        return Err(FileServiceError::other(format!(
            "Image preview '{}' filesystem candidate '{}' does not match descriptor '{}'",
            descriptor.path,
            candidate.filesystem_kind,
            descriptor.filesystem_kind.as_deref().unwrap_or_default()
        )));
    }
    Ok(candidate)
}

pub fn preview_partition_candidate_from_record(
    partition: &crate::partition_query_service::PartitionRecord,
) -> PreviewPartitionCandidate {
    PreviewPartitionCandidate {
        partition_index: partition.partition_index as usize,
        filesystem_kind: partition
            .filesystem
            .as_deref()
            .unwrap_or(&partition.kind_label)
            .to_string(),
        offset: partition.offset,
        lvm_identity: preview_lvm_identity_from_record(partition),
    }
}

pub(crate) fn preview_lvm_identity_from_datasource(
    identity: &crate::datasource_service::LvmLogicalVolumeIdentity,
) -> crate::file_service::viewer::PreviewLvmIdentity {
    crate::file_service::viewer::PreviewLvmIdentity {
        vg_uuid: identity.vg_uuid.clone(),
        vg_name: identity.vg_name.clone(),
        lv_uuid: identity.lv_uuid.clone(),
        lv_name: identity.lv_name.clone(),
        pv_offsets: identity.pv_offsets.clone(),
        pv_sources: identity
            .pv_sources
            .iter()
            .map(
                |source| crate::file_service::viewer::PreviewLvmPhysicalVolumeSource {
                    source_path: source.source_path.clone(),
                    source_kind: source
                        .source_kind
                        .as_ref()
                        .map(std::string::ToString::to_string)
                        .unwrap_or_default(),
                    offset: source.offset,
                    pv_uuid: source.pv_uuid.clone(),
                    pv_name: source.pv_name.clone(),
                },
            )
            .collect(),
    }
}

fn preview_lvm_identity_from_record(
    partition: &crate::partition_query_service::PartitionRecord,
) -> Option<crate::file_service::viewer::PreviewLvmIdentity> {
    let offsets_json = partition.lvm_pv_offsets_json.as_deref()?;
    let pv_offsets = serde_json::from_str::<Vec<u64>>(offsets_json).ok()?;
    if pv_offsets.is_empty() {
        return None;
    }
    let pv_sources = partition
        .lvm_pv_sources_json
        .as_deref()
        .and_then(|json| {
            serde_json::from_str::<
                    Vec<crate::file_service::viewer::PreviewLvmPhysicalVolumeSource>,
                >(json)
                .ok()
        })
        .unwrap_or_default();
    let lv_uuid = partition.lvm_lv_uuid.clone().unwrap_or_default();
    let lv_name = partition.lvm_lv_name.clone().unwrap_or_default();
    if lv_uuid.is_empty() && lv_name.is_empty() {
        return None;
    }
    Some(crate::file_service::viewer::PreviewLvmIdentity {
        vg_uuid: partition.lvm_vg_uuid.clone().unwrap_or_default(),
        vg_name: partition.lvm_vg_name.clone().unwrap_or_default(),
        lv_uuid,
        lv_name,
        pv_offsets,
        pv_sources,
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/file_service/partition.rs"]
mod tests;
