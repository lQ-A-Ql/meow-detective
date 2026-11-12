use super::ImageFilesystemCandidate;
use std::collections::HashMap;

/// Assign effective partition indices for candidates where `partition_index` is `None`
/// (typical for MBR disks). Candidates are sorted by offset so that indices are
/// deterministic and consistent across probe, import, and viewer paths.
///
/// Returns a map from candidate position to effective index. Candidates that already
/// have a `partition_index` are left unchanged (not included in the map).
pub fn assign_effective_partition_indices(
    candidates: &[ImageFilesystemCandidate],
) -> HashMap<usize, usize> {
    let mut offsets: Vec<(usize, u64)> = candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| c.partition_index.is_none())
        .map(|(i, c)| (i, c.offset))
        .collect();
    offsets.sort_by_key(|(_, o)| *o);
    let mut map = HashMap::new();
    for (unique_idx, (orig_pos, _)) in offsets.iter().enumerate() {
        map.insert(*orig_pos, unique_idx);
    }
    map
}

/// Resolve the effective partition index for a candidate, using the precomputed
/// map from `assign_effective_partition_indices`.
pub fn effective_partition_index(
    candidate: &ImageFilesystemCandidate,
    candidate_pos: usize,
    index_map: &HashMap<usize, usize>,
) -> usize {
    candidate
        .partition_index
        .unwrap_or_else(|| *index_map.get(&candidate_pos).unwrap_or(&0))
}
