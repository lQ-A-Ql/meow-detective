//! Bounded EVTX structured-event extraction integrated into the analysis pipeline.
//!
//! This adapter turns `artifacts_windows::extract_structured_events` output into
//! `Artifact` and `TimelineEvent` records that are persisted alongside
//! Registry/Browser/Email extractions.  It persists boot/shutdown, Security,
//! and Application events as typed artifacts so that `EventLogPanel` can query
//! them by artifact family.

use crate::analysis_service::artifact_builders::{base_attrs, make_artifact, make_timeline_event};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::ExtractionOutcome;
use artifacts_windows::evtx::EvtxBootError;
use artifacts_windows::{
    EvtxApplicationEvent, EvtxApplicationEventKind, EvtxBootEvent, EvtxBootEventKind,
    EvtxSecurityEvent, EvtxSecurityEventKind, EvtxStructuredEvent, EvtxStructuredExtraction,
};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::BTreeMap;

const EVTX_EXTRACTOR_ID: &str = "evtx.structured";

pub fn extract_evtx_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
) -> Result<ExtractionOutcome, EvtxBootError> {
    let extraction = artifacts_windows::extract_structured_events(bytes, &candidate.path)?;
    Ok(project_evtx_extraction(candidate, extraction))
}

fn project_evtx_extraction(
    candidate: &EvidenceCandidate,
    extraction: EvtxStructuredExtraction,
) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    outcome.warnings.extend(extraction.warnings.iter().cloned());
    extract_boot_events(candidate, &extraction, &mut outcome);
    extract_security_events(candidate, &extraction, &mut outcome);
    extract_application_events(candidate, &extraction, &mut outcome);
    outcome
}

fn extract_boot_events(
    candidate: &EvidenceCandidate,
    extraction: &EvtxStructuredExtraction,
    outcome: &mut ExtractionOutcome,
) {
    for event in &extraction.boot_events {
        project_boot_event(candidate, event, outcome);
    }
}

pub(super) fn project_evtx_event(
    candidate: &EvidenceCandidate,
    event: &EvtxStructuredEvent,
    outcome: &mut ExtractionOutcome,
) {
    match event {
        EvtxStructuredEvent::Boot(event) => project_boot_event(candidate, event, outcome),
        EvtxStructuredEvent::Security(event) => project_security_event(candidate, event, outcome),
        EvtxStructuredEvent::Application(event) => {
            project_application_event(candidate, event, outcome)
        }
    }
}

fn project_boot_event(
    candidate: &EvidenceCandidate,
    event: &EvtxBootEvent,
    outcome: &mut ExtractionOutcome,
) {
    let attrs = boot_event_attrs(candidate, event);
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
        let event_type = match event.kind {
            EvtxBootEventKind::OperatingSystemStarted | EvtxBootEventKind::EventLogStarted => {
                "Boot"
            }
            EvtxBootEventKind::OperatingSystemShutdown
            | EvtxBootEventKind::EventLogStopped
            | EvtxBootEventKind::PlannedShutdown
            | EvtxBootEventKind::UnexpectedShutdown => "Shutdown",
            _ => event.kind.as_str(),
        };
        let timeline_event = make_timeline_event(
            &candidate.file_id,
            event_type,
            timestamp,
            title,
            event.note.clone(),
            attrs,
            EVTX_EXTRACTOR_ID,
        );
        outcome.timeline_events.push(timeline_event);
    }
}

fn extract_security_events(
    candidate: &EvidenceCandidate,
    extraction: &EvtxStructuredExtraction,
    outcome: &mut ExtractionOutcome,
) {
    for event in &extraction.security_events {
        project_security_event(candidate, event, outcome);
    }
}

fn project_security_event(
    candidate: &EvidenceCandidate,
    event: &EvtxSecurityEvent,
    outcome: &mut ExtractionOutcome,
) {
    let attrs = security_event_attrs(candidate, event);
    let title = format!(
        "Security {} ({})",
        event.kind.as_str(),
        event.target_user.as_deref().unwrap_or("-")
    );
    let note = security_event_note(event);
    let artifact = make_artifact(
        "EvtxSecurityEvent",
        title.clone(),
        note.clone(),
        candidate,
        EVTX_EXTRACTOR_ID,
        attrs.clone(),
    );
    outcome.artifacts.push(artifact);

    if let Ok(timestamp) = parse_event_timestamp(&event.timestamp) {
        let timeline_event = make_timeline_event(
            &candidate.file_id,
            "Security",
            timestamp,
            title,
            note,
            attrs,
            EVTX_EXTRACTOR_ID,
        );
        outcome.timeline_events.push(timeline_event);
    }
}

fn extract_application_events(
    candidate: &EvidenceCandidate,
    extraction: &EvtxStructuredExtraction,
    outcome: &mut ExtractionOutcome,
) {
    for event in &extraction.application_events {
        project_application_event(candidate, event, outcome);
    }
}

fn project_application_event(
    candidate: &EvidenceCandidate,
    event: &EvtxApplicationEvent,
    outcome: &mut ExtractionOutcome,
) {
    let attrs = application_event_attrs(candidate, event);
    let title = format!(
        "Application {} ({})",
        event.kind.as_str(),
        event.application.as_deref().unwrap_or("-")
    );
    let note = application_event_note(event);
    let artifact = make_artifact(
        "EvtxApplicationEvent",
        title.clone(),
        note.clone(),
        candidate,
        EVTX_EXTRACTOR_ID,
        attrs.clone(),
    );
    outcome.artifacts.push(artifact);

    if let Ok(timestamp) = parse_event_timestamp(&event.timestamp) {
        let event_type = match event.kind {
            EvtxApplicationEventKind::ApplicationCrash => "ApplicationCrash",
            EvtxApplicationEventKind::ApplicationHang => "ApplicationHang",
            EvtxApplicationEventKind::WindowsErrorReporting => "WindowsErrorReporting",
            EvtxApplicationEventKind::SoftwareInstallation => "SoftwareInstallation",
        };
        let timeline_event = make_timeline_event(
            &candidate.file_id,
            event_type,
            timestamp,
            title,
            note,
            attrs,
            EVTX_EXTRACTOR_ID,
        );
        outcome.timeline_events.push(timeline_event);
    }
}

fn boot_event_attrs(
    candidate: &EvidenceCandidate,
    event: &EvtxBootEvent,
) -> BTreeMap<String, Value> {
    let mut attrs = base_attrs(candidate);
    attrs.insert(
        "timestamp".to_string(),
        Value::String(event.timestamp.clone()),
    );
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
    insert_details(&mut attrs, &event.details);
    attrs
}

fn security_event_attrs(
    candidate: &EvidenceCandidate,
    event: &EvtxSecurityEvent,
) -> BTreeMap<String, Value> {
    let mut attrs = base_attrs(candidate);
    attrs.insert(
        "timestamp".to_string(),
        Value::String(event.timestamp.clone()),
    );
    attrs.insert(
        "eventId".to_string(),
        Value::Number(serde_json::Number::from(event.event_id)),
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
    attrs.insert(
        "kind".to_string(),
        Value::String(event.kind.as_str().to_string()),
    );
    insert_optional(&mut attrs, "targetUser", event.target_user.clone());
    insert_optional(&mut attrs, "subjectUser", event.subject_user.clone());
    insert_optional(&mut attrs, "logonType", event.logon_type.clone());
    insert_optional(&mut attrs, "ipAddress", event.ip_address.clone());
    insert_optional(&mut attrs, "workstation", event.workstation.clone());
    insert_optional(&mut attrs, "failureReason", event.failure_reason.clone());
    insert_optional(&mut attrs, "processName", event.process_name.clone());
    insert_optional(
        &mut attrs,
        "parentProcessName",
        event.parent_process_name.clone(),
    );
    insert_optional(&mut attrs, "taskName", event.task_name.clone());
    insert_optional(&mut attrs, "privilegeList", event.privilege_list.clone());
    insert_optional(&mut attrs, "memberName", event.member_name.clone());
    insert_details(&mut attrs, &event.details);
    attrs
}

fn security_event_note(event: &EvtxSecurityEvent) -> String {
    match event.kind {
        EvtxSecurityEventKind::LogonSuccess => format!(
            "User {} logged on from {} via {}",
            event.target_user.as_deref().unwrap_or("-"),
            event.ip_address.as_deref().unwrap_or("local"),
            event.logon_type.as_deref().unwrap_or("-")
        ),
        EvtxSecurityEventKind::LogonFailure => format!(
            "Failed logon for {} from {}: {}",
            event.target_user.as_deref().unwrap_or("-"),
            event.ip_address.as_deref().unwrap_or("local"),
            event.failure_reason.as_deref().unwrap_or("unknown")
        ),
        EvtxSecurityEventKind::ExplicitCredentials => format!(
            "Explicit credentials used by {} for {}",
            event.subject_user.as_deref().unwrap_or("-"),
            event.target_user.as_deref().unwrap_or("-")
        ),
        EvtxSecurityEventKind::ProcessCreated => format!(
            "Process created: {}",
            event.process_name.as_deref().unwrap_or("-")
        ),
        EvtxSecurityEventKind::ScheduledTaskCreated => format!(
            "Scheduled task created: {}",
            event.task_name.as_deref().unwrap_or("-")
        ),
        EvtxSecurityEventKind::ScheduledTaskModified => format!(
            "Scheduled task modified: {}",
            event.task_name.as_deref().unwrap_or("-")
        ),
        EvtxSecurityEventKind::AccountCreated => format!(
            "Account created: {}",
            event.target_user.as_deref().unwrap_or("-")
        ),
        EvtxSecurityEventKind::GroupMemberAdded => format!(
            "Member {} added to group {}",
            event.member_name.as_deref().unwrap_or("-"),
            event.target_user.as_deref().unwrap_or("-")
        ),
    }
}

fn application_event_attrs(
    candidate: &EvidenceCandidate,
    event: &EvtxApplicationEvent,
) -> BTreeMap<String, Value> {
    let mut attrs = base_attrs(candidate);
    attrs.insert(
        "timestamp".to_string(),
        Value::String(event.timestamp.clone()),
    );
    attrs.insert(
        "eventId".to_string(),
        Value::Number(serde_json::Number::from(event.event_id)),
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
    attrs.insert(
        "kind".to_string(),
        Value::String(event.kind.as_str().to_string()),
    );
    insert_optional(&mut attrs, "application", event.application.clone());
    insert_optional(&mut attrs, "faultModule", event.fault_module.clone());
    insert_optional(&mut attrs, "productName", event.product_name.clone());
    insert_optional(&mut attrs, "manufacturer", event.manufacturer.clone());
    insert_details(&mut attrs, &event.details);
    attrs
}

fn application_event_note(event: &EvtxApplicationEvent) -> String {
    match event.kind {
        EvtxApplicationEventKind::ApplicationCrash => format!(
            "Application crash: {}",
            event.application.as_deref().unwrap_or("-")
        ),
        EvtxApplicationEventKind::ApplicationHang => format!(
            "Application hang: {}",
            event.application.as_deref().unwrap_or("-")
        ),
        EvtxApplicationEventKind::WindowsErrorReporting => format!(
            "Windows Error Reporting for {}",
            event.application.as_deref().unwrap_or("-")
        ),
        EvtxApplicationEventKind::SoftwareInstallation => format!(
            "Software installed: {} by {}",
            event.product_name.as_deref().unwrap_or("-"),
            event.manufacturer.as_deref().unwrap_or("-")
        ),
    }
}

fn insert_optional(attrs: &mut BTreeMap<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        attrs.insert(key.to_string(), Value::String(value));
    }
}

fn insert_details(
    attrs: &mut BTreeMap<String, Value>,
    details: &std::collections::BTreeMap<String, String>,
) {
    if !details.is_empty() {
        if let Ok(value) = serde_json::to_value(details) {
            attrs.insert("details".to_string(), value);
        }
    }
}

fn parse_event_timestamp(raw: &str) -> Result<DateTime<Utc>, AnalysisServiceError> {
    if raw == "unknown" {
        return Err(AnalysisServiceError::Other("unknown timestamp".to_string()));
    }
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| AnalysisServiceError::Other(format!("parse EVTX timestamp {raw}: {err}")))
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/evtx.rs"]
mod tests;
