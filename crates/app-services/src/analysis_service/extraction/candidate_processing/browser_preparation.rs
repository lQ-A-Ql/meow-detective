use super::super::browser::{
    is_sqlite_path,
    persistence::{self, PersistenceContext},
};
use super::*;

pub(super) fn prepare<F>(
    coordinator: &mut CandidateCoordinator<'_, '_, '_, F>,
    candidate: EvidenceCandidate,
    capability: AnalysisCapability,
    bytes: Vec<u8>,
) -> Result<PreparedWork<PreparedCandidate, CandidateCompletion>, AnalysisServiceError>
where
    F: FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
{
    if !is_sqlite_path(&candidate.path) {
        unreachable!("only SQLite browser artifacts are prepared here");
    }
    if bytes.len() as u64 != candidate.size {
        return Ok(source_preparation::ready_deferred(
            candidate,
            capability,
            "browser database input was truncated".into(),
        ));
    }
    let companions = match read_companions(coordinator, &candidate, capability) {
        Ok(companions) => companions,
        Err(CandidateExtractionError::Warning(warning)) => {
            return Ok(source_preparation::ready_deferred(
                candidate, capability, warning,
            ))
        }
        Err(CandidateExtractionError::Cancelled) => return Err(AnalysisServiceError::Cancelled),
    };
    let context = PersistenceContext {
        connection: coordinator.context.conn,
        case_id: coordinator.context.case_id,
        capability,
        preload: coordinator.context.preloads.browser,
        cancel: coordinator.context.cancel_token,
    };
    match persistence::persist(context, &candidate, &bytes, &companions) {
        Ok(outcome) => Ok(source_preparation::ready_persisted(
            candidate, capability, outcome,
        )),
        Err(
            error @ (AnalysisServiceError::Extraction(_) | AnalysisServiceError::Unsupported(_)),
        ) => Ok(source_preparation::ready_deferred(
            candidate,
            capability,
            error.to_string(),
        )),
        Err(error) => Err(error),
    }
}

fn read_companions<F>(
    coordinator: &mut CandidateCoordinator<'_, '_, '_, F>,
    primary: &EvidenceCandidate,
    capability: AnalysisCapability,
) -> Result<Vec<ArtifactCompanion>, CandidateExtractionError>
where
    F: FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
{
    let mut loaded = Vec::with_capacity(primary.companions.len());
    for companion in &primary.companions {
        let limit = if companion.path.to_ascii_lowercase().ends_with("-wal") {
            artifacts_browser::sqlite::MAX_WAL_BYTES
        } else {
            8 * 1024 * 1024
        };
        if companion.encrypted || companion.size > limit as u64 {
            return Err(CandidateExtractionError::Warning(
                "browser companion is encrypted or exceeds the byte limit".into(),
            ));
        }
        let candidate = EvidenceCandidate {
            file_id: companion.file_id.clone(),
            data_source_id: primary.data_source_id.clone(),
            partition_index: primary.partition_index,
            path: companion.path.clone(),
            size: companion.size,
            encrypted: companion.encrypted,
            content_identity: companion.content_identity.clone(),
            companions: Vec::new(),
            evidence_kind: "browser-companion".into(),
            parser: primary.parser.clone(),
            category: primary.category.clone(),
            modified_at: primary.modified_at,
        };
        let data = read_candidate_bytes_with_progress(
            &candidate,
            limit + 1,
            coordinator.context.cancel_token,
            coordinator.file_reader,
            |_| {},
        )?;
        if data.len() as u64 != companion.size {
            return Err(CandidateExtractionError::Warning(
                "browser companion input was truncated".into(),
            ));
        }
        coordinator
            .progress
            .report_read_progress(capability, primary, data.len(), limit);
        loaded.push(ArtifactCompanion {
            file_id: companion.file_id.clone(),
            file_path: companion.path.clone(),
            data,
        });
    }
    Ok(loaded)
}
