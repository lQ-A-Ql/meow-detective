use super::super::{ImageFilesystemCandidate, LvmPhysicalVolumeSource};
use domain::DataSourceKind;
use std::path::Path;

pub(crate) fn lvm_source_fingerprint(source_path: &str) -> String {
    if source_path.is_empty() {
        return "<unknown>".to_string();
    }

    let digest = <sha2::Sha256 as sha2::Digest>::digest(source_path.as_bytes());
    let short_hash = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("path-sha256:{short_hash}")
}

pub(crate) fn normalize_lvm_uuid_for_match(uuid: &str) -> String {
    uuid.trim()
        .chars()
        .filter(|ch| *ch != '-')
        .collect::<String>()
        .to_ascii_lowercase()
}

pub(in crate::datasource_service) fn lvm_pv_source_key(
    source: &LvmPhysicalVolumeSource,
) -> (String, u64, String) {
    (
        format!(
            "{}|{}",
            source
                .source_kind
                .as_ref()
                .map(std::string::ToString::to_string)
                .unwrap_or_default(),
            source.source_path
        ),
        source.offset,
        normalize_lvm_uuid_for_match(&source.pv_uuid),
    )
}

pub(super) fn lvm_source_path_key(source_path: &Path) -> String {
    std::fs::canonicalize(source_path)
        .unwrap_or_else(|_| source_path.to_path_buf())
        .to_string_lossy()
        .to_string()
        .to_ascii_lowercase()
}

pub(super) fn lvm_source_matches(
    source: &LvmPhysicalVolumeSource,
    source_path: &Path,
    source_kind: &DataSourceKind,
) -> bool {
    source
        .source_kind
        .as_ref()
        .is_none_or(|kind| kind == source_kind)
        && lvm_source_path_key(Path::new(&source.source_path)) == lvm_source_path_key(source_path)
}

pub(super) fn representative_lvm_candidate<'a>(
    lvm_indices: &'a [(usize, ImageFilesystemCandidate)],
    pv_offsets: &[u64],
) -> Option<&'a ImageFilesystemCandidate> {
    for offset in pv_offsets {
        if let Some((_, candidate)) = lvm_indices
            .iter()
            .find(|(_, candidate)| candidate.offset == *offset)
        {
            return Some(candidate);
        }
    }
    None
}

pub(super) fn lvm_sources_for_pv_mappings(
    sources: &[LvmPhysicalVolumeSource],
    pv_mappings: &[(String, u64)],
    pv_offsets: &[u64],
) -> Vec<LvmPhysicalVolumeSource> {
    if !pv_mappings.is_empty() {
        return pv_mappings
            .iter()
            .filter_map(|(pv_name, offset)| {
                sources
                    .iter()
                    .find(|source| {
                        source.offset == *offset
                            && source.pv_name.as_deref().is_none_or(|name| name == pv_name)
                    })
                    .cloned()
                    .map(|mut source| {
                        source.pv_name = Some(pv_name.clone());
                        source
                    })
            })
            .collect();
    }

    pv_offsets
        .iter()
        .filter_map(|offset| sources.iter().find(|source| source.offset == *offset))
        .cloned()
        .collect()
}
