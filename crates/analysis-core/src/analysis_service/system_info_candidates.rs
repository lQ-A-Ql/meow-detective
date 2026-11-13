use crate::analysis_service::candidates::{
    evidence_candidates_for_categories, normalize_evidence_path, EvidenceCandidate,
};
use crate::analysis_service::error::AnalysisServiceError;
use domain::FileEntry;
use persistence_source::repositories::file_repo::FileRepo;
use rusqlite::Connection;

#[derive(Default)]
pub(super) struct SystemInfoCandidates {
    pub(super) system_hive: Option<FileEntry>,
    pub(super) software_hive: Option<FileEntry>,
    pub(super) system_evtx: Option<FileEntry>,
    pub(super) system_input: Option<EvidenceCandidate>,
    pub(super) software_input: Option<EvidenceCandidate>,
}

pub(super) fn find_system_info_candidates(
    conn: &Connection,
) -> Result<SystemInfoCandidates, AnalysisServiceError> {
    let entries = evidence_candidates_for_categories(conn, &["Registry", "EventLogs"])?;
    let system_input = unique_candidate(&entries, "/windows/system32/config/system")?;
    let software_input = unique_candidate(&entries, "/windows/system32/config/software")?;
    if let (Some(system), Some(software)) = (&system_input, &software_input) {
        if system.data_source_id != software.data_source_id
            || system.partition_index != software.partition_index
        {
            return Err(AnalysisServiceError::Unsupported(
                "system information requires hives from one Windows installation".into(),
            ));
        }
    }
    let scope = system_input.as_ref().or(software_input.as_ref());
    let event_inputs = entries
        .into_iter()
        .filter(|candidate| {
            scope.is_none_or(|scope| {
                candidate.data_source_id == scope.data_source_id
                    && candidate.partition_index == scope.partition_index
            })
        })
        .collect::<Vec<_>>();
    let event = unique_candidate(&event_inputs, "/windows/system32/winevt/logs/system.evtx")?;
    let repository = FileRepo::new(conn);
    Ok(SystemInfoCandidates {
        system_hive: system_input
            .as_ref()
            .map(|candidate| repository.find_by_id(&candidate.file_id))
            .transpose()?
            .flatten(),
        software_hive: software_input
            .as_ref()
            .map(|candidate| repository.find_by_id(&candidate.file_id))
            .transpose()?
            .flatten(),
        system_evtx: event
            .as_ref()
            .map(|candidate| repository.find_by_id(&candidate.file_id))
            .transpose()?
            .flatten(),
        system_input,
        software_input,
    })
}

fn unique_candidate(
    candidates: &[EvidenceCandidate],
    suffix: &str,
) -> Result<Option<EvidenceCandidate>, AnalysisServiceError> {
    let mut matching = candidates.iter().filter(|candidate| {
        !candidate.encrypted && normalize_evidence_path(&candidate.path).ends_with(suffix)
    });
    let first = matching.next().cloned();
    if matching.next().is_some() {
        return Err(AnalysisServiceError::Unsupported(
            "multiple Windows system-information inputs are ambiguous".into(),
        ));
    }
    Ok(first)
}
