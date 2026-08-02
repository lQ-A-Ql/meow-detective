use std::collections::{BTreeSet, HashMap};

use domain::DataSourcePlatform;
use persistence_sqlite::repositories::{
    file_repo::FileRepo, filesystem_locator_repo::FilesystemLocatorRepo,
    partition_repo::PartitionRepo,
};
use rusqlite::Connection;

use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};

type LocatorKey = (String, usize, String);

pub(super) fn order_candidates_for_extraction(
    conn: &Connection,
    platform: DataSourcePlatform,
    candidates: &mut [EvidenceCandidate],
) {
    let inode_hints = if platform == DataSourcePlatform::Linux {
        load_derived_xfs_inode_hints(conn, candidates)
    } else {
        HashMap::new()
    };
    candidates.sort_by_cached_key(|candidate| {
        let normalized_path = normalize_evidence_path(&candidate.path);
        let inode = candidate.partition_index.and_then(|partition_index| {
            inode_hints
                .get(&(
                    candidate.data_source_id.clone(),
                    partition_index,
                    normalized_path.clone(),
                ))
                .copied()
        });
        (
            candidate.data_source_id.clone(),
            candidate.partition_index.unwrap_or(usize::MAX),
            inode.is_none(),
            inode.unwrap_or(u64::MAX),
            normalized_path,
            candidate.file_id.0.clone(),
            candidate.category.clone(),
        )
    });
}

fn load_derived_xfs_inode_hints(
    conn: &Connection,
    candidates: &[EvidenceCandidate],
) -> HashMap<LocatorKey, u64> {
    let source_ids = candidates
        .iter()
        .map(|candidate| candidate.data_source_id.clone())
        .collect::<BTreeSet<_>>();
    let file_repo = FileRepo::new(conn);
    let partition_repo = PartitionRepo::new(conn);
    let locator_repo = FilesystemLocatorRepo::new(conn);
    let catalog_fingerprint = match crate::derived_source_catalog::load_catalog_fingerprint(conn) {
        Ok(Some(fingerprint)) => fingerprint,
        Ok(None) | Err(_) => return HashMap::new(),
    };
    let mut hints = HashMap::new();

    for source_id in source_ids {
        if !is_derived_source(&file_repo, &source_id) {
            continue;
        }
        let needed_partitions = candidates
            .iter()
            .filter(|candidate| candidate.data_source_id == source_id)
            .filter_map(|candidate| candidate.partition_index)
            .collect::<BTreeSet<_>>();
        let Ok(partitions) = partition_repo.find_by_data_source(&source_id) else {
            continue;
        };
        for partition in partitions {
            let Ok(partition_index) = usize::try_from(partition.partition_index) else {
                continue;
            };
            if !needed_partitions.contains(&partition_index) {
                continue;
            }
            let filesystem_kind = partition
                .filesystem
                .as_deref()
                .unwrap_or(&partition.kind_label);
            if !filesystem_kind.eq_ignore_ascii_case("xfs") {
                continue;
            }
            let locator_candidate: crate::file_service::PreviewPartitionCandidate =
                crate::file_service::preview_partition_candidate_from_record(&partition);
            let Ok(locator_scope) =
                crate::file_service::filesystem_locators::derived_filesystem_locator_scope(
                    &catalog_fingerprint,
                    &locator_candidate,
                )
            else {
                continue;
            };
            let Ok(locators) = locator_repo.list_file_locators(
                &source_id,
                partition_index,
                filesystem_kind,
                &locator_scope,
            ) else {
                continue;
            };
            add_unambiguous_inode_hints(
                &mut hints,
                &source_id,
                partition_index,
                locators.into_iter().filter_map(|locator| {
                    locator
                        .locator
                        .parse::<u64>()
                        .ok()
                        .map(|inode| (locator.path, inode))
                }),
            );
        }
    }
    hints
}

fn is_derived_source(file_repo: &FileRepo<'_>, source_id: &str) -> bool {
    file_repo
        .find_data_source_location(&domain::DataSourceId(source_id.to_string()))
        .ok()
        .flatten()
        .is_some_and(|(kind, _)| kind == "ceph_rbd")
}

fn add_unambiguous_inode_hints(
    hints: &mut HashMap<LocatorKey, u64>,
    source_id: &str,
    partition_index: usize,
    locators: impl IntoIterator<Item = (String, u64)>,
) {
    let mut ambiguous = BTreeSet::new();
    for (path, inode) in locators {
        if inode == 0 {
            continue;
        }
        let key = (
            source_id.to_string(),
            partition_index,
            normalize_evidence_path(&path),
        );
        if hints.get(&key).is_some_and(|existing| *existing != inode) {
            ambiguous.insert(key);
        } else {
            hints.entry(key).or_insert(inode);
        }
    }
    for key in ambiguous {
        hints.remove(&key);
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/candidate_order.rs"]
mod tests;
