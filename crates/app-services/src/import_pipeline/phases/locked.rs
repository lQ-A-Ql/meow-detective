use crate::{datasource_service, staging};

/// A locked BitLocker volume has a valid catalog root but cannot be opened by
/// a filesystem reader until a verified key is registered. Treat it as a
/// completed metadata-only partition during the ordinary image import so the
/// import can reach `ready` and the BitLocker service can inspect/unlock it.
///
/// The placeholder root is seeded by `persist_probe`; catalog enumeration is
/// intentionally deferred to `import_unlocked_bitlocker_catalog` after unlock.
pub(crate) fn mark_locked_partitions_done(
    manifest: &mut staging::StagingManifest,
    candidates: &[datasource_service::ImageFilesystemCandidate],
) {
    let index_map = datasource_service::assign_effective_partition_indices(candidates);
    for (ordinal, candidate) in candidates.iter().enumerate() {
        if candidate.kind != datasource_service::ImageFilesystemKind::BitLocker {
            continue;
        }
        let index = datasource_service::effective_partition_index(candidate, ordinal, &index_map);
        let Some(partition) = manifest
            .partitions
            .iter_mut()
            .find(|partition| partition.index == index)
        else {
            continue;
        };
        partition.status = staging::PartitionStatus::Done;
        partition.file_count = 0;
        partition.dir_count = 0;
        partition.total_size = 0;
        partition.last_path = None;
        partition.completed_at = Some(chrono::Utc::now().to_rfc3339());
        partition.error = None;
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/import_pipeline/phases/locked.rs"]
mod tests;
