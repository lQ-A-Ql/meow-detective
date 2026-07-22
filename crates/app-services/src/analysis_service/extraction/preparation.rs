use super::artifact_query::already_has_v1_artifacts;
use super::candidate_processing::{capability_for_candidate, CandidateSource, ExistingCheckpoints};
use super::registry_preload::{preload_registry_context, RegistryPreloadContext};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::capability::AnalysisCapability;
use domain::FileEntryId;
use rusqlite::Connection;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::atomic::AtomicBool;

pub(super) fn prepare_registry_preload(
    conn: &Connection,
    candidates: &[EvidenceCandidate],
    selected: &[AnalysisCapability],
    checkpoints: &ExistingCheckpoints<'_>,
    cancel_token: &AtomicBool,
    file_reader: &mut impl FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
) -> Result<RegistryPreloadContext, crate::analysis_service::error::AnalysisServiceError> {
    let candidates_by_id = candidates
        .iter()
        .map(|candidate| (candidate.file_id.0.as_str(), candidate))
        .collect::<HashMap<_, _>>();
    let mut registry_reader = |file_id: &FileEntryId, read_limit: usize| {
        let candidate = candidates_by_id
            .get(file_id.0.as_str())
            .ok_or_else(|| format!("analysis candidate '{}' was not discovered", file_id.0))?;
        file_reader(candidate, read_limit).map(|source| match source {
            CandidateSource::Reader(reader) => reader,
            CandidateSource::Bytes(bytes) => Box::new(Cursor::new(bytes)) as Box<dyn Read>,
        })
    };
    preload_registry_context(
        conn,
        candidates,
        cancel_token,
        &mut registry_reader,
        |candidate| {
            let checkpoint_exists = capability_for_candidate(selected, candidate)
                .is_some_and(|capability| checkpoints.has_candidate(candidate, capability));
            if checkpoint_exists {
                Ok(true)
            } else if checkpoints.storage_available {
                Ok(false)
            } else {
                already_has_v1_artifacts(conn, candidate)
            }
        },
    )
}
