use super::{
    AnalysisCapability, AnalysisServiceError, CandidateCoordinator, CandidateProcessingContext,
    CandidateProgressResult, EvidenceCandidate, PersistedExtractionOutcome, PLUGIN_CAPABILITY_KEY,
};
use crate::analysis_service::extraction::state::{
    AnalysisOutputProducer, AnalysisOutputReplacement,
};
use crate::analysis_service::extraction::PluginExtractFailure;
use std::time::Instant;
use transport::dto::AnalysisWorkStageDto;

pub(super) fn replacements(
    context: &CandidateProcessingContext<'_>,
    candidate: &EvidenceCandidate,
    capability: AnalysisCapability,
) -> Vec<AnalysisOutputReplacement> {
    let producers = if capability.key == PLUGIN_CAPABILITY_KEY {
        super::super::plugin::matching_producer_ids(context.plugins, candidate)
            .into_iter()
            .map(AnalysisOutputProducer::ExtractorId)
            .collect()
    } else {
        vec![AnalysisOutputProducer::Prefix(capability.producer_prefix())]
    };
    producers
        .into_iter()
        .map(|producer| AnalysisOutputReplacement {
            source_object_id: candidate.file_id.0.clone(),
            producer,
        })
        .collect()
}

pub(super) fn preserve_failed_candidate(
    context: &CandidateProcessingContext<'_>,
    candidate: &EvidenceCandidate,
    capability: AnalysisCapability,
) -> Result<(), AnalysisServiceError> {
    context.output.preserve_replacements(
        &replacements(context, candidate, capability),
        context.cancel_token,
    )?;
    Ok(())
}

pub(super) fn apply_outcome<F>(
    coordinator: &mut CandidateCoordinator<'_, '_, '_, F>,
    candidate: &EvidenceCandidate,
    capability: AnalysisCapability,
    observation: PersistedExtractionOutcome,
    plugin_failures: Vec<PluginExtractFailure>,
) -> Result<CandidateProgressResult, AnalysisServiceError> {
    let context = coordinator.context;
    let scopes = replacements(context, candidate, capability);
    let started = Instant::now();
    let mut retired = 0;
    let result = CandidateProgressResult {
        artifact_count: observation.artifact_count,
        timeline_event_count: observation.timeline_event_count,
        warning: !observation.warnings.is_empty(),
        ..CandidateProgressResult::default()
    };
    if capability.key == PLUGIN_CAPABILITY_KEY {
        for scope in scopes {
            let failed = matches!(&scope.producer, AnalysisOutputProducer::ExtractorId(id)
                if plugin_failures.iter().any(|failure| failure.plugin_id == *id));
            if failed {
                context
                    .output
                    .preserve_replacements(&[scope], context.cancel_token)?;
            } else {
                retired += context
                    .output
                    .complete_replacements(&[scope], context.cancel_token)?;
            }
        }
        coordinator
            .state
            .record_persisted_plugin_outcome(capability, observation, plugin_failures);
    } else if observation.artifact_count == 0
        && observation.timeline_event_count == 0
        && !observation.warnings.is_empty()
        && context
            .output
            .preserve_replacements(&scopes, context.cancel_token)?
            > 0
    {
        coordinator
            .state
            .record_retained_failure(capability, observation.warnings);
    } else {
        retired += context
            .output
            .complete_replacements(&scopes, context.cancel_token)?;
        coordinator
            .state
            .record_persisted_outcome(capability, candidate, observation);
    }
    coordinator.progress.report_work(
        AnalysisWorkStageDto::ReplacementDelete,
        started,
        retired,
        0,
        true,
    );
    Ok(result)
}

pub(super) fn retire_blocked_candidate<F>(
    coordinator: &mut CandidateCoordinator<'_, '_, '_, F>,
    candidate: &EvidenceCandidate,
    capability: AnalysisCapability,
    warning: String,
) -> Result<CandidateProgressResult, AnalysisServiceError> {
    let context = coordinator.context;
    context.output.complete_replacements(
        &replacements(context, candidate, capability),
        context.cancel_token,
    )?;
    coordinator.state.record_warning(capability, warning);
    Ok(CandidateProgressResult {
        warning: true,
        ..CandidateProgressResult::default()
    })
}
