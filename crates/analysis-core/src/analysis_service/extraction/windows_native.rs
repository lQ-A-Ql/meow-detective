use super::ExtractionOutcome;
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::capability::{find_capability, WINDOWS_NATIVE_EXTRACTOR_VERSION};
use artifacts_core::{ArtifactContext, ArtifactExtractor, VecSink};
use domain::{Artifact, TimelineEvent};
use std::io::Cursor;

pub(super) fn extract(candidate: &EvidenceCandidate, bytes: &[u8]) -> ExtractionOutcome {
    let Some(capability) = find_capability(&candidate.category) else {
        unreachable!("native Windows candidates have a registered capability");
    };
    let mut outcome = ExtractionOutcome::default();
    if bytes.len() as u64 != candidate.size {
        outcome.warnings.push(format!(
            "{} native Windows input was truncated: expected {} bytes, received {}",
            candidate.path,
            candidate.size,
            bytes.len()
        ));
        return outcome;
    }
    let extractors = extractors_for(&candidate.category);
    let mut matched = false;
    for extractor in extractors
        .iter()
        .filter(|value| value.supports_path(&candidate.path))
    {
        matched = true;
        let mut sink = VecSink::new();
        let context = ArtifactContext {
            file_id: candidate.file_id.clone(),
            file_path: candidate.path.clone(),
            reader: Box::new(Cursor::new(bytes.to_vec())),
        };
        match extractor.run(context, &mut sink) {
            Ok(report) => {
                if sink.artifacts.is_empty()
                    && sink.timeline_events.is_empty()
                    && report.errors.is_empty()
                {
                    outcome.warnings.push(format!(
                        "{} {} produced no recognized records",
                        candidate.path,
                        extractor.id()
                    ));
                }
                let producer = format!("{}{}", capability.producer_prefix(), extractor.id());
                for artifact in &mut sink.artifacts {
                    stamp_artifact(artifact, candidate, &producer);
                }
                for event in &mut sink.timeline_events {
                    stamp_event(event, candidate, &producer);
                }
                outcome.artifacts.extend(sink.artifacts);
                outcome.timeline_events.extend(sink.timeline_events);
                outcome.warnings.extend(
                    report
                        .errors
                        .into_iter()
                        .map(|error| format!("{} {}: {error}", candidate.path, extractor.id())),
                );
            }
            Err(error) => {
                outcome
                    .warnings
                    .push(format!("{} {}: {error}", candidate.path, extractor.id()))
            }
        }
    }
    if !matched {
        outcome.warnings.push(format!(
            "{} has no matching native Windows extractor",
            candidate.path
        ));
    }
    outcome
}

fn extractors_for(category: &str) -> Vec<Box<dyn ArtifactExtractor>> {
    match category {
        "ProgramExecution" => vec![Box::new(artifacts_windows::PrefetchExtractor)],
        "UserActivity" => vec![
            Box::new(artifacts_windows::LnkExtractor),
            Box::new(artifacts_windows::JumpListExtractor),
        ],
        "RecycleBin" => vec![Box::new(artifacts_windows::RecycleBinExtractor)],
        "Thumbnails" => vec![Box::new(artifacts_windows::ThumbcacheExtractor)],
        "ResourceUsage" => vec![Box::new(artifacts_windows::SruExtractor)],
        _ => Vec::new(),
    }
}

fn stamp_artifact(artifact: &mut Artifact, candidate: &EvidenceCandidate, producer: &str) {
    artifact.source_object_id = Some(candidate.file_id.clone());
    artifact.extractor_id = Some(producer.to_string());
    artifact.extractor_version = Some(WINDOWS_NATIVE_EXTRACTOR_VERSION.to_string());
    artifact.source_attribution = Some(candidate.path.clone());
    artifact
        .attrs
        .extend(crate::analysis_service::artifact_builders::base_attrs(
            candidate,
        ));
}

fn stamp_event(event: &mut TimelineEvent, candidate: &EvidenceCandidate, producer: &str) {
    event.source_object_id = candidate.file_id.0.clone();
    event.parser_id = Some(producer.to_string());
    event.parser_version = Some(WINDOWS_NATIVE_EXTRACTOR_VERSION.to_string());
    event.source_attribution = Some(candidate.path.clone());
    event
        .attrs
        .extend(crate::analysis_service::artifact_builders::base_attrs(
            candidate,
        ));
    event.event_type = match event.event_type.as_str() {
        "PROGRAM_EXECUTION" => "FILE_EXECUTED",
        "LINK_CREATED" => "FILE_CREATED",
        "LINK_MODIFIED" => "FILE_MODIFIED",
        "LINK_ACCESSED" => "FILE_ACCESSED",
        other => other,
    }
    .to_string();
}
