use crate::analysis_service::error::AnalysisServiceError;
use domain::FileEntry;
use rusqlite::Connection;

#[derive(Default)]
pub(super) struct SystemInfoCandidates {
    pub(super) system_hive: Option<FileEntry>,
    pub(super) software_hive: Option<FileEntry>,
    pub(super) system_evtx: Option<FileEntry>,
}

pub(super) fn find_system_info_candidates(
    conn: &Connection,
) -> Result<SystemInfoCandidates, AnalysisServiceError> {
    let entries = crate::analysis_service::collect_file_entries(conn)?;
    let mut candidates = SystemInfoCandidates::default();
    for entry in entries.into_iter().filter(FileEntry::is_file) {
        let path = entry.path.to_ascii_lowercase().replace('\\', "/");
        select_candidate(
            &mut candidates.system_hive,
            entry.clone(),
            &path,
            "windows/system32/config/system",
        );
        select_candidate(
            &mut candidates.software_hive,
            entry.clone(),
            &path,
            "windows/system32/config/software",
        );
        select_candidate(
            &mut candidates.system_evtx,
            entry,
            &path,
            "windows/system32/winevt/logs/system.evtx",
        );
    }
    Ok(candidates)
}

fn select_candidate(
    selected: &mut Option<FileEntry>,
    entry: FileEntry,
    normalized_path: &str,
    suffix: &str,
) {
    if !normalized_path.ends_with(suffix)
        || selected
            .as_ref()
            .is_some_and(|current| current.path.len() <= entry.path.len())
    {
        return;
    }
    *selected = Some(entry);
}
