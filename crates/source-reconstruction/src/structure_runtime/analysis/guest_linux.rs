use analysis_core::analysis_service::{
    evidence_candidates_for_categories, extract_linux_candidate, AnalysisServiceError,
    EvidenceCandidate, ExtractionOutcome, MAX_ANALYSIS_SOURCE_BYTES,
};
use domain::{
    structure::{
        CancellationScope, CapabilityKind, ConflictReason, EvidenceRequirement, MetadataFailure,
        ResourceKind, SourceRevisionRef, TargetFailure,
    },
    CaseId, DataSourceId, DataSourcePlatform,
};
use evidence_access::file_service::{
    read_file_bytes_for_case, FileServiceError, SourceReadContext,
};
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::{
    repositories::{datasource_repo::DataSourceRepo, source_revision_repo::SourceRevisionRepo},
    DbError,
};
use serde_json::{json, Value};

use crate::structure_runtime::{ExecutionContext, RuntimeError};

pub(super) fn analyze(
    context: &ExecutionContext<'_>,
    source: &SourceRevisionRef,
) -> Result<Value, RuntimeError> {
    validate_source(context, source)?;
    check_active(context)?;
    let source_id = DataSourceId(source.source_id.clone());
    let case_id = CaseId(source.case_id.clone());
    let control = open_linux_control(context, &source_id)?;
    let candidates = evidence_candidates_for_categories(&control, &["LinuxArtifacts"])
        .map_err(analysis_error)?;
    check_active(context)?;
    if candidates.is_empty() {
        return Err(missing_file_range());
    }
    check_limit(
        candidates.len() as u64,
        u64::from(context.request.resource_policy.page_rows),
        ResourceKind::ActiveWork,
    )?;
    let mut reader = SourceReadContext::new(
        &control,
        context.connection,
        context.case_root,
        &case_id,
        &source_id,
    );
    let mut candidates_output = Vec::with_capacity(candidates.len());
    let mut rows = 0u64;
    let mut output_bytes = 0u64;
    for candidate in &candidates {
        let (value, records) = analyze_candidate(context, source, &mut reader, candidate)?;
        rows = rows.checked_add(records).ok_or(RuntimeError::Capacity)?;
        check_limit(
            rows,
            u64::from(context.request.resource_policy.page_rows),
            ResourceKind::ActiveWork,
        )?;
        output_bytes = output_bytes
            .checked_add(serialized_bytes(&value)?)
            .ok_or(RuntimeError::Capacity)?;
        check_limit(
            output_bytes,
            context.request.resource_policy.page_bytes,
            ResourceKind::Bytes,
        )?;
        candidates_output.push(value);
    }
    validate_source(context, source)?;
    check_active(context)?;
    let output = json!({
        "analysis": "guest-linux",
        "sourceRevision": source,
        "platform": "linux",
        "candidateCount": candidates.len(),
        "recordCount": rows,
        "candidates": candidates_output,
        "diagnostics": [],
    });
    check_limit(
        serialized_bytes(&output)?,
        context.request.resource_policy.page_bytes,
        ResourceKind::Bytes,
    )?;
    Ok(output)
}

fn open_linux_control(
    context: &ExecutionContext<'_>,
    source_id: &DataSourceId,
) -> Result<rusqlite::Connection, RuntimeError> {
    let storage = DataSourceRepo::new(context.connection)
        .find_storage(source_id)
        .map_err(database_error)?
        .ok_or_else(missing_file_range)?;
    if DataSourcePlatform::parse_explicit(&storage.platform) != Ok(DataSourcePlatform::Linux) {
        return Err(unsupported());
    }
    if !matches!(storage.import_state.as_str(), "ready" | "ready_metadata") {
        return Err(RuntimeError::Target(TargetFailure::MissingEvidence {
            requirement: EvidenceRequirement::NamespacePage,
        }));
    }
    source_kernel::source_db::open_registered_source_db_read_only(
        context.connection,
        context.case_root,
        source_id,
    )
    .map_err(database_error)
}

fn analyze_candidate(
    context: &ExecutionContext<'_>,
    source: &SourceRevisionRef,
    reader: &mut SourceReadContext<'_>,
    candidate: &EvidenceCandidate,
) -> Result<(Value, u64), RuntimeError> {
    check_active(context)?;
    if candidate.data_source_id != source.source_id || candidate.category != "LinuxArtifacts" {
        return Err(RuntimeError::InvalidDefinition);
    }
    let entry = FileRepo::new(reader.source_connection())
        .find_by_id(&candidate.file_id)
        .map_err(database_error)?
        .ok_or_else(missing_file_range)?;
    if entry.data_source_id.0 != source.source_id
        || entry.size != Some(candidate.size)
        || entry.path != candidate.path
    {
        return Err(input_changed());
    }
    if candidate.encrypted || entry.encrypted {
        return Err(RuntimeError::Target(TargetFailure::MissingEvidence {
            requirement: EvidenceRequirement::DecryptionMaterial,
        }));
    }
    let bytes = read_candidate(context, reader, candidate)?;
    let digest = infrastructure::hashing::sha256_bytes(&bytes);
    if entry
        .hash_sha256
        .as_ref()
        .is_some_and(|expected| expected != &digest)
    {
        return Err(corrupt(MetadataFailure::InvalidChecksum));
    }
    check_active(context)?;
    let outcome = extract_linux_candidate(candidate, &bytes);
    check_active(context)?;
    validate_outcome(candidate, &outcome)?;
    let rows = outcome
        .artifacts
        .len()
        .checked_add(outcome.timeline_events.len())
        .ok_or(RuntimeError::Capacity)? as u64;
    check_limit(
        rows,
        u64::from(context.request.resource_policy.page_rows),
        ResourceKind::ActiveWork,
    )?;
    Ok((
        json!({
            "sourceId": candidate.data_source_id,
            "fileId": candidate.file_id.0,
            "path": candidate.path,
            "partitionIndex": candidate.partition_index,
            "size": bytes.len(),
            "sha256": digest,
            "contentIdentity": candidate.content_identity,
            "evidenceKind": candidate.evidence_kind,
            "parser": candidate.parser,
            "artifacts": outcome.artifacts,
            "events": outcome.timeline_events,
            "diagnosticCounts": outcome.diagnostic_counts,
            "diagnostics": [],
        }),
        rows,
    ))
}

fn read_candidate(
    context: &ExecutionContext<'_>,
    reader: &mut SourceReadContext<'_>,
    candidate: &EvidenceCandidate,
) -> Result<Vec<u8>, RuntimeError> {
    let limit = context
        .request
        .resource_policy
        .bytes
        .min(MAX_ANALYSIS_SOURCE_BYTES as u64);
    check_limit(candidate.size, limit, ResourceKind::Bytes)?;
    if candidate.size == 0 {
        return Err(missing_file_range());
    }
    let capacity = usize::try_from(candidate.size).map_err(|_| RuntimeError::Capacity)?;
    let mut bytes = Vec::with_capacity(capacity);
    while (bytes.len() as u64) < candidate.size {
        check_active(context)?;
        let offset = bytes.len() as u64;
        let length = (candidate.size - offset)
            .min(infrastructure::constants::MAX_RANGE_LENGTH as u64) as u32;
        let page = read_file_bytes_for_case(&mut *reader, &candidate.file_id, offset, length)
            .map_err(file_error)?;
        if page.len() != length as usize {
            return Err(corrupt(MetadataFailure::Truncated));
        }
        bytes.extend_from_slice(&page);
    }
    check_active(context)?;
    Ok(bytes)
}

fn validate_outcome(
    candidate: &EvidenceCandidate,
    outcome: &ExtractionOutcome,
) -> Result<(), RuntimeError> {
    let counts = outcome.diagnostic_counts;
    if counts.read_failures != 0 {
        return Err(RuntimeError::Target(TargetFailure::IoUnavailable));
    }
    if counts.corrupt_inputs != 0 {
        return Err(corrupt(MetadataFailure::InvalidStructure));
    }
    if counts.capability_limits != 0 || counts.recovered_inputs != 0 || !outcome.warnings.is_empty()
    {
        return Err(unsupported());
    }
    if counts.empty_inputs != 0
        || outcome.artifacts.is_empty() && outcome.timeline_events.is_empty()
    {
        return Err(missing_file_range());
    }
    if outcome
        .artifacts
        .iter()
        .any(|artifact| artifact.source_object_id.as_ref() != Some(&candidate.file_id))
        || outcome
            .timeline_events
            .iter()
            .any(|event| event.source_object_id != candidate.file_id.0)
    {
        return Err(RuntimeError::InvalidOutput);
    }
    Ok(())
}

fn validate_source(
    context: &ExecutionContext<'_>,
    source: &SourceRevisionRef,
) -> Result<(), RuntimeError> {
    let capture = context
        .request
        .source_scope
        .iter()
        .find(|capture| capture.source == *source)
        .filter(|_| source.case_id == context.request.case_id)
        .ok_or(RuntimeError::InvalidDefinition)?;
    let (current, content) = SourceRevisionRepo::new(context.connection)
        .current(&source.case_id, &source.source_id)
        .map_err(database_error)?;
    if current != *source || content.acquisition_id != capture.acquisition_id {
        return Err(input_changed());
    }
    Ok(())
}

fn check_active(context: &ExecutionContext<'_>) -> Result<(), RuntimeError> {
    source_kernel::source_runtime::range_view::check_active(
        context.cancel,
        context.request.resource_policy.deadline_unix_ms,
    )
    .map_err(super::super::proof_state::classify)
}

fn check_limit(required: u64, limit: u64, resource: ResourceKind) -> Result<(), RuntimeError> {
    if required > limit {
        return Err(RuntimeError::Target(TargetFailure::ResourceLimit {
            resource,
            required,
            limit,
        }));
    }
    Ok(())
}

fn analysis_error(error: AnalysisServiceError) -> RuntimeError {
    match error {
        AnalysisServiceError::Db(error) => database_error(error),
        AnalysisServiceError::Io(error) => super::super::proof_state::classify(error),
        AnalysisServiceError::Cancelled => cancelled(),
        AnalysisServiceError::Unsupported(_) => unsupported(),
        AnalysisServiceError::CorruptInput(_) | AnalysisServiceError::Extraction(_) => {
            corrupt(MetadataFailure::InvalidStructure)
        }
        AnalysisServiceError::NotFound(_, _) => missing_file_range(),
        _ => RuntimeError::InvalidOutput,
    }
}

fn file_error(error: FileServiceError) -> RuntimeError {
    match error {
        FileServiceError::Db(error) => database_error(error),
        FileServiceError::Io(error) => super::super::proof_state::classify(error),
        FileServiceError::Integrity(_) => corrupt(MetadataFailure::InvalidChecksum),
        FileServiceError::NotFound(_) => missing_file_range(),
        FileServiceError::Unsupported(_) => unsupported(),
        _ => RuntimeError::InvalidOutput,
    }
}

fn database_error(error: DbError) -> RuntimeError {
    match error {
        DbError::ReadCancelled => cancelled(),
        DbError::ReadTimedOut => RuntimeError::Target(TargetFailure::DeadlineExceeded),
        DbError::Io(error) => super::super::proof_state::classify(error),
        error => RuntimeError::Database(error),
    }
}

fn corrupt(reason: MetadataFailure) -> RuntimeError {
    RuntimeError::Target(TargetFailure::CorruptMetadata { reason })
}

fn input_changed() -> RuntimeError {
    RuntimeError::Target(TargetFailure::Conflicted {
        reason: ConflictReason::InputChanged,
    })
}

fn cancelled() -> RuntimeError {
    RuntimeError::Target(TargetFailure::Cancelled {
        scope: CancellationScope::Target,
    })
}

fn unsupported() -> RuntimeError {
    RuntimeError::Target(TargetFailure::UnsupportedFeature {
        capability: CapabilityKind::FileRange,
    })
}

fn missing_file_range() -> RuntimeError {
    RuntimeError::Target(TargetFailure::MissingCapabilities {
        capabilities: [CapabilityKind::FileRange].into_iter().collect(),
    })
}

fn serialized_bytes(value: &Value) -> Result<u64, RuntimeError> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len() as u64)
        .map_err(|_| RuntimeError::InvalidOutput)
}
