//! Bounded EVTX boot/shutdown extraction integrated into the analysis pipeline.
//!
//! This adapter turns `artifacts_windows::extract_boot_shutdown_events` output
//! into `Artifact` and `TimelineEvent` records that are persisted alongside
//! Registry/Browser/Email extractions.

use crate::analysis_service::artifact_builders::{base_attrs, make_artifact, make_timeline_event};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::ExtractionOutcome;
use chrono::{DateTime, Utc};
use serde_json::Value;

const EVTX_EXTRACTOR_ID: &str = "evtx.boot_shutdown";

pub fn extract_evtx_candidate(candidate: &EvidenceCandidate, bytes: &[u8]) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();

    match artifacts_windows::extract_boot_shutdown_events(bytes, &candidate.path) {
        Ok(extraction) => {
            outcome.warnings.extend(extraction.warnings);
            for event in extraction.events {
                let mut attrs = base_attrs(candidate);
                attrs.insert(
                    "eventId".to_string(),
                    Value::Number(serde_json::Number::from(event.event_id)),
                );
                attrs.insert(
                    "eventKind".to_string(),
                    Value::String(event.kind.as_str().to_string()),
                );
                if let Some(record_id) = event.record_id {
                    attrs.insert(
                        "recordId".to_string(),
                        Value::Number(serde_json::Number::from(record_id)),
                    );
                }
                if let Some(provider) = &event.provider {
                    attrs.insert("provider".to_string(), Value::String(provider.clone()));
                }
                attrs.insert("note".to_string(), Value::String(event.note.clone()));
                if let Some(ref details) = event.details {
                    attrs.insert("details".to_string(), Value::String(details.clone()));
                }

                let title = format!("EVTX {} event {}", event.kind.as_str(), event.event_id);
                let artifact = make_artifact(
                    "EvtxBootShutdown",
                    title.clone(),
                    event.note.clone(),
                    candidate,
                    EVTX_EXTRACTOR_ID,
                    attrs.clone(),
                );
                outcome.artifacts.push(artifact);

                if let Ok(timestamp) = parse_event_timestamp(&event.timestamp) {
                    let event_type = match event.event_id {
                        6005 => "Boot",
                        _ => "Shutdown",
                    };
                    let timeline_event = make_timeline_event(
                        &candidate.file_id,
                        event_type,
                        timestamp,
                        title,
                        event.note,
                        attrs,
                        EVTX_EXTRACTOR_ID,
                    );
                    outcome.timeline_events.push(timeline_event);
                }
            }
        }
        Err(err) => outcome.warnings.push(err.to_string()),
    }

    outcome
}

fn parse_event_timestamp(raw: &str) -> Result<DateTime<Utc>, AnalysisServiceError> {
    if raw == "unknown" {
        return Err(AnalysisServiceError::Other("unknown timestamp".to_string()));
    }
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| AnalysisServiceError::Other(format!("parse EVTX timestamp {raw}: {err}")))
}
