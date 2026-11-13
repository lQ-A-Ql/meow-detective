use crate::analysis_service::{
    candidates::EvidenceCandidate, AnalysisServiceError, MAX_REGISTRY_ANALYSIS_BYTES,
};
use artifacts_windows::registry::{RegistryHiveSource, RegistrySnapshot};
use domain::FileEntryId;

pub(super) fn snapshot<E: std::fmt::Display>(
    candidate: &EvidenceCandidate,
    read: &mut impl FnMut(&FileEntryId, usize) -> Result<Vec<u8>, E>,
) -> Result<RegistrySnapshot<'static>, AnalysisServiceError> {
    let bytes = read_input(
        &candidate.file_id,
        candidate.size,
        candidate.encrypted,
        read,
    )?;
    let mut logs = Vec::with_capacity(2);
    for suffix in [".log1", ".log2"] {
        let path = format!(
            "{}{suffix}",
            crate::analysis_service::normalize_evidence_path(&candidate.path)
        );
        let companion = candidate.companions.iter().find(|companion| {
            crate::analysis_service::normalize_evidence_path(&companion.path) == path
        });
        logs.push(
            companion
                .map(|companion| {
                    read_input(
                        &companion.file_id,
                        companion.size,
                        companion.encrypted,
                        read,
                    )
                })
                .transpose()?,
        );
    }
    let snapshot = RegistrySnapshot::from_owned(bytes, logs[0].as_deref(), logs[1].as_deref())
        .map_err(|error| AnalysisServiceError::Extraction(error.to_string()))?;
    snapshot
        .registry_view()
        .map_err(|error| AnalysisServiceError::Extraction(error.to_string()))?;
    Ok(snapshot)
}

fn read_input<E: std::fmt::Display>(
    id: &FileEntryId,
    size: u64,
    encrypted: bool,
    read: &mut impl FnMut(&FileEntryId, usize) -> Result<Vec<u8>, E>,
) -> Result<Vec<u8>, AnalysisServiceError> {
    if encrypted || size > MAX_REGISTRY_ANALYSIS_BYTES as u64 {
        return Err(AnalysisServiceError::Unsupported(
            "registry input exceeds its readable scope".into(),
        ));
    }
    let bytes = read(id, MAX_REGISTRY_ANALYSIS_BYTES)
        .map_err(|error| AnalysisServiceError::Read(error.to_string()))?;
    if bytes.len() as u64 != size {
        return Err(AnalysisServiceError::Read(
            "registry input was truncated or changed".into(),
        ));
    }
    Ok(bytes)
}
