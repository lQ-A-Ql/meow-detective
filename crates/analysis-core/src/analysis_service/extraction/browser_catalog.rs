use crate::analysis_service::candidates::preparation::locator;
use crate::analysis_service::candidates::{
    normalize_evidence_path, EvidenceCandidate, PreparationCatalog,
};
use uuid::Uuid;

pub(super) fn locate_master_key_files(
    catalog: &PreparationCatalog,
    data_source_id: &str,
) -> Vec<EvidenceCandidate> {
    catalog
        .files(data_source_id)
        .filter(|record| is_master_key_path(&record.path))
        .map(locator)
        .collect()
}

fn is_master_key_path(path: &str) -> bool {
    let normalized = normalize_evidence_path(path);
    let Some(name) = normalized.rsplit('/').next() else {
        return false;
    };
    if Uuid::parse_str(name).is_err() || !normalized.contains("/microsoft/protect/") {
        return false;
    }
    let parent_is_sid = normalized
        .split('/')
        .rev()
        .nth(1)
        .is_some_and(|sid| sid.starts_with("s-1-"));
    let system_protect = normalized.contains("/microsoft/protect/s-1-5-18/user/")
        || normalized.contains("/microsoft/protect/s-1-5-18/machine/");
    parent_is_sid || system_protect
}
