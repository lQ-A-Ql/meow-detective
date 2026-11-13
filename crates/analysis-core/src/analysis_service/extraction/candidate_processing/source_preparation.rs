use super::*;
use crate::analysis_service::capability::CandidateReadPolicy;
use crate::analysis_service::extraction::linux::linux_candidate_read_limit;
use crate::analysis_service::extraction::reader::read_candidate_source_with_progress;

const MAX_PLUGIN_INPUT_BYTES: usize = 128 * 1024 * 1024;

pub(super) fn estimated_input_weight(item: &CandidateWorkItem) -> usize {
    if item.capability.read_policy == CandidateReadPolicy::RegistryPreload
        || item.candidate.encrypted
        || is_unsupported_linux_candidate(&item.candidate)
    {
        return 0;
    }
    let read_limit = candidate_read_limit(&item.candidate, item.capability);
    let primary = usize::try_from(item.candidate.size.min(read_limit as u64)).unwrap_or(read_limit);
    item.candidate
        .companions
        .iter()
        .fold(primary, |weight, companion| {
            weight.saturating_add(usize::try_from(companion.size).unwrap_or(usize::MAX))
        })
}

fn candidate_read_limit(candidate: &EvidenceCandidate, capability: AnalysisCapability) -> usize {
    if capability.key == "BrowserHistory" && super::super::browser::is_sqlite_path(&candidate.path)
    {
        return artifacts_browser::sqlite::MAX_DATABASE_BYTES;
    }
    if capability.read_policy == CandidateReadPolicy::LinuxPathAware
        && crate::analysis_service::extraction::containerd_images::is_metadata_path(
            &normalize_evidence_path(&candidate.path),
        )
    {
        return 32 * 1024 * 1024;
    }
    match capability.read_policy {
        CandidateReadPolicy::Bounded(limit) => limit,
        CandidateReadPolicy::Full => MAX_PLUGIN_INPUT_BYTES.saturating_add(1),
        CandidateReadPolicy::LinuxPathAware => {
            linux_candidate_read_limit(&normalize_evidence_path(&candidate.path))
        }
        CandidateReadPolicy::RegistryPreload => 0,
    }
}

pub(super) fn prepare_source<F>(
    coordinator: &mut CandidateCoordinator<'_, '_, '_, F>,
    candidate: EvidenceCandidate,
    capability: AnalysisCapability,
) -> Result<PreparedWork<PreparedCandidate, CandidateCompletion>, AnalysisServiceError>
where
    F: FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
{
    let read_limit = candidate_read_limit(&candidate, capability);
    if capability.read_policy == CandidateReadPolicy::Full
        && plugin_input_exceeds_budget(&candidate)
    {
        return Ok(ready_deferred(
            candidate,
            capability,
            format!("plugin input and companions exceed the {MAX_PLUGIN_INPUT_BYTES} byte budget"),
        ));
    }
    if candidate.category == "EventLogs" {
        return prepare_evtx_source(coordinator, candidate, capability, read_limit);
    }

    let bytes = match read_candidate_bytes(coordinator, &candidate, capability, read_limit) {
        Ok(bytes) => bytes,
        Err(CandidateExtractionError::Warning(warning)) => {
            return Ok(ready_warning(candidate, capability, warning));
        }
        Err(CandidateExtractionError::Cancelled) => {
            return Err(AnalysisServiceError::Cancelled);
        }
    };
    if capability.read_policy == CandidateReadPolicy::Full && bytes.len() as u64 != candidate.size {
        let warning = format!(
            "plugin input length mismatch: catalog={} bytes, read={} bytes",
            candidate.size,
            bytes.len()
        );
        return Ok(ready_warning(candidate, capability, warning));
    }
    if capability.read_policy == CandidateReadPolicy::LinuxPathAware
        && crate::analysis_service::extraction::containerd_images::is_metadata_path(
            &normalize_evidence_path(&candidate.path),
        )
    {
        let outcome = crate::analysis_service::extraction::containerd_images::extract(
            coordinator.context.conn,
            &candidate,
            &bytes,
            coordinator.context.cancel_token,
        )?;
        return Ok(PreparedWork::Parallel {
            input: PreparedCandidate {
                candidate,
                capability,
                input: PreparedCandidateInput::Extraction(outcome),
            },
            weight_bytes: 0,
        });
    }
    if capability.key == "BrowserHistory" && super::super::browser::is_sqlite_path(&candidate.path)
    {
        return super::browser_preparation::prepare(coordinator, candidate, capability, bytes);
    }
    let input = if capability.key == PLUGIN_CAPABILITY_KEY {
        let companions = match read_plugin_companions(coordinator, &candidate, capability) {
            Ok(companions) => companions,
            Err(CandidateExtractionError::Warning(warning)) => {
                return Ok(ready_warning(candidate, capability, warning));
            }
            Err(CandidateExtractionError::Cancelled) => {
                return Err(AnalysisServiceError::Cancelled);
            }
        };
        PreparedCandidateInput::Plugin {
            primary: bytes,
            companions,
        }
    } else {
        PreparedCandidateInput::Bytes(bytes)
    };
    let weight_bytes = prepared_input_bytes(&input);
    Ok(PreparedWork::Parallel {
        input: PreparedCandidate {
            candidate,
            capability,
            input,
        },
        weight_bytes,
    })
}

fn prepared_input_bytes(input: &PreparedCandidateInput) -> usize {
    match input {
        PreparedCandidateInput::Plugin {
            primary,
            companions,
        } => companions.iter().fold(primary.len(), |total, companion| {
            total.saturating_add(companion.data.len())
        }),
        PreparedCandidateInput::Bytes(bytes) => bytes.len(),
        PreparedCandidateInput::Registry | PreparedCandidateInput::Extraction(_) => 0,
        PreparedCandidateInput::Browser { .. } => {
            unreachable!("browser candidates use the dedicated preparation path")
        }
    }
}

fn plugin_input_exceeds_budget(candidate: &EvidenceCandidate) -> bool {
    candidate
        .companions
        .iter()
        .try_fold(candidate.size, |total, companion| {
            total.checked_add(companion.size)
        })
        .is_none_or(|size| size > MAX_PLUGIN_INPUT_BYTES as u64)
}

fn read_plugin_companions<F>(
    coordinator: &mut CandidateCoordinator<'_, '_, '_, F>,
    primary: &EvidenceCandidate,
    capability: AnalysisCapability,
) -> Result<Vec<ArtifactCompanion>, CandidateExtractionError>
where
    F: FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
{
    let mut loaded = Vec::with_capacity(primary.companions.len());
    for companion in &primary.companions {
        let candidate = EvidenceCandidate {
            file_id: companion.file_id.clone(),
            data_source_id: primary.data_source_id.clone(),
            partition_index: primary.partition_index,
            path: companion.path.clone(),
            size: companion.size,
            encrypted: companion.encrypted,
            content_identity: companion.content_identity.clone(),
            companions: Vec::new(),
            evidence_kind: "plugin-companion".to_string(),
            parser: primary.parser.clone(),
            category: primary.category.clone(),
            modified_at: primary.modified_at,
        };
        let limit = usize::try_from(companion.size)
            .unwrap_or(usize::MAX)
            .saturating_add(1);
        let data = read_candidate_bytes_with_progress(
            &candidate,
            limit,
            coordinator.context.cancel_token,
            coordinator.file_reader,
            |_| {},
        )?;
        if data.len() as u64 != companion.size {
            return Err(CandidateExtractionError::Warning(format!(
                "plugin companion length mismatch for {}: catalog={} bytes, read={} bytes",
                companion.path,
                companion.size,
                data.len()
            )));
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

fn prepare_evtx_source<F>(
    coordinator: &mut CandidateCoordinator<'_, '_, '_, F>,
    candidate: EvidenceCandidate,
    capability: AnalysisCapability,
    read_limit: usize,
) -> Result<PreparedWork<PreparedCandidate, CandidateCompletion>, AnalysisServiceError>
where
    F: FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
{
    let source = match (coordinator.file_reader)(&candidate, read_limit) {
        Ok(source) => source,
        Err(error) => {
            let warning = format!("{} read failed: {error}", candidate.path);
            return Ok(ready_deferred(candidate, capability, warning));
        }
    };
    let source = match source {
        CandidateSource::Seekable(reader) => {
            let result = super::super::evtx::persist_reader(
                coordinator.context.output,
                &candidate,
                reader,
                coordinator.context.cancel_token,
            );
            let kind = match result {
                Ok(observation) => CandidateCompletionKind::Persisted {
                    observation,
                    plugin_failures: Vec::new(),
                },
                Err(AnalysisServiceError::Cancelled) => {
                    return Err(AnalysisServiceError::Cancelled)
                }
                Err(error) => CandidateCompletionKind::Deferred(error.to_string()),
            };
            return Ok(PreparedWork::Ready(CandidateCompletion {
                candidate,
                capability,
                kind,
            }));
        }
        source => source,
    };
    let probe_limit = read_limit.saturating_add(1);
    let bytes = match read_candidate_source_with_progress(
        &candidate,
        source,
        probe_limit,
        coordinator.context.cancel_token,
        |bytes_read| {
            coordinator.progress.report_read_progress(
                capability,
                &candidate,
                bytes_read.min(read_limit),
                read_limit,
            );
        },
    ) {
        Ok(bytes) => bytes,
        Err(CandidateExtractionError::Warning(warning)) => {
            return Ok(ready_deferred(candidate, capability, warning));
        }
        Err(CandidateExtractionError::Cancelled) => {
            return Err(AnalysisServiceError::Cancelled);
        }
    };
    if bytes.len() > read_limit || candidate.size > read_limit as u64 {
        let warning = format!(
            "{} exceeds the {} byte buffered EVTX limit and the evidence reader is not seekable",
            candidate.path, read_limit
        );
        return Ok(ready_deferred(candidate, capability, warning));
    }
    let weight_bytes = bytes.len();
    Ok(PreparedWork::Parallel {
        input: PreparedCandidate {
            candidate,
            capability,
            input: PreparedCandidateInput::Bytes(bytes),
        },
        weight_bytes,
    })
}

fn read_candidate_bytes<F>(
    coordinator: &mut CandidateCoordinator<'_, '_, '_, F>,
    candidate: &EvidenceCandidate,
    capability: AnalysisCapability,
    read_limit: usize,
) -> Result<Vec<u8>, CandidateExtractionError>
where
    F: FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
{
    read_candidate_bytes_with_progress(
        candidate,
        read_limit,
        coordinator.context.cancel_token,
        coordinator.file_reader,
        |bytes_read| {
            coordinator
                .progress
                .report_read_progress(capability, candidate, bytes_read, read_limit);
        },
    )
}

pub(super) fn ready_deferred(
    candidate: EvidenceCandidate,
    capability: AnalysisCapability,
    warning: String,
) -> PreparedWork<PreparedCandidate, CandidateCompletion> {
    PreparedWork::Ready(CandidateCompletion {
        candidate,
        capability,
        kind: CandidateCompletionKind::Deferred(warning),
    })
}

fn ready_warning(
    candidate: EvidenceCandidate,
    capability: AnalysisCapability,
    warning: String,
) -> PreparedWork<PreparedCandidate, CandidateCompletion> {
    PreparedWork::Ready(CandidateCompletion {
        candidate,
        capability,
        kind: CandidateCompletionKind::Warning(warning),
    })
}
