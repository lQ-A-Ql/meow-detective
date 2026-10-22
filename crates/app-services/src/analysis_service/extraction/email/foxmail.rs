//! Foxmail native `.box` extraction.

use super::super::ExtractionOutcome;
use super::eml::{extract_parsed_email, parse_email_message};
use crate::analysis_service::candidates::EvidenceCandidate;
use containers_pst::foxmail::parse_box;
use serde_json::Value;

const FOXMAIL_EXTRACTOR_ID: &str = "email.foxmail_box";

pub(super) fn extract_foxmail_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
) -> ExtractionOutcome {
    let parsed_box = parse_box(bytes);
    let mut outcome = ExtractionOutcome::default();
    if parsed_box.separator_count == 0 {
        outcome.warnings.push(format!(
            "Foxmail .box file {} has no recognized record separator",
            candidate.path
        ));
        return outcome;
    }
    if parsed_box.ignored_record_count > 0 {
        outcome.warnings.push(format!(
            "Foxmail .box file {} skipped {} unrecognized record(s)",
            candidate.path, parsed_box.ignored_record_count
        ));
    }
    for (index, record) in parsed_box.records.into_iter().enumerate() {
        match parse_email_message(&record) {
            Ok(parsed) => {
                let mut parsed_outcome =
                    extract_parsed_email(candidate, parsed, FOXMAIL_EXTRACTOR_ID);
                add_foxmail_metadata(&mut parsed_outcome, candidate, index as u64 + 1);
                outcome.artifacts.extend(parsed_outcome.artifacts);
                outcome
                    .timeline_events
                    .extend(parsed_outcome.timeline_events);
                outcome.warnings.extend(parsed_outcome.warnings);
            }
            Err(error) => outcome.warnings.push(format!(
                "Foxmail .box record {} in {} could not be parsed as RFC822: {}",
                index + 1,
                candidate.path,
                error
            )),
        }
    }
    outcome
}

fn add_foxmail_metadata(
    outcome: &mut ExtractionOutcome,
    candidate: &EvidenceCandidate,
    message_index: u64,
) {
    for artifact in &mut outcome.artifacts {
        artifact.attrs.insert(
            "mailClient".to_string(),
            Value::String("Foxmail".to_string()),
        );
        artifact.attrs.insert(
            "containerPath".to_string(),
            Value::String(candidate.path.clone()),
        );
        artifact.attrs.insert(
            "messageIndex".to_string(),
            Value::Number(message_index.into()),
        );
    }
    for event in &mut outcome.timeline_events {
        event.attrs.insert(
            "mailClient".to_string(),
            Value::String("Foxmail".to_string()),
        );
        event.attrs.insert(
            "containerPath".to_string(),
            Value::String(candidate.path.clone()),
        );
        event.attrs.insert(
            "messageIndex".to_string(),
            Value::Number(message_index.into()),
        );
    }
}
