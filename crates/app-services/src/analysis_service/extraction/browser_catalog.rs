use super::browser_preload::locator_from_entry;
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use crate::analysis_service::AnalysisServiceError;
use rusqlite::Connection;
use uuid::Uuid;

pub(super) fn locate_master_key_files(
    conn: &Connection,
    data_source_id: &str,
) -> Result<Vec<EvidenceCandidate>, AnalysisServiceError> {
    let mut candidates = Vec::new();
    persistence_source::repositories::file_repo::FileRepo::new(conn).visit_all(
        Some(data_source_id),
        |entry| {
            if is_master_key_path(&entry.path) && !entry.deleted && !entry.is_directory() {
                candidates.push(locator_from_entry(data_source_id, &entry, "BrowserPreload"));
            }
            Ok(())
        },
    )?;
    Ok(candidates)
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
