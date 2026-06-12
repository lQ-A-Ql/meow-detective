use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::ANALYSIS_EXTRACTOR_VERSION;
use chrono::{DateTime, Utc};
use domain::{Artifact, ArtifactId, FileEntryId, TimelineEvent, TimelineEventId};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;

pub(crate) fn make_artifact(
    family: &str,
    title: String,
    summary: String,
    candidate: &EvidenceCandidate,
    extractor_id: &str,
    attrs: BTreeMap<String, Value>,
) -> Artifact {
    Artifact {
        id: ArtifactId(Uuid::new_v4().to_string()),
        family: family.to_string(),
        title,
        summary,
        source_object_id: Some(candidate.file_id.clone()),
        extractor_id: Some(extractor_id.to_string()),
        extractor_version: Some(ANALYSIS_EXTRACTOR_VERSION.to_string()),
        confidence: Some(0.85),
        source_attribution: Some(candidate.path.clone()),
        created_at: Utc::now(),
        attrs,
    }
}

pub(crate) fn make_timeline_event(
    source_id: &FileEntryId,
    event_type: &str,
    timestamp: DateTime<Utc>,
    title: String,
    description: String,
    attrs: BTreeMap<String, Value>,
    parser_id: &str,
) -> TimelineEvent {
    TimelineEvent {
        id: TimelineEventId(Uuid::new_v4().to_string()),
        source_object_id: source_id.0.clone(),
        event_type: event_type.to_string(),
        timestamp,
        title,
        description,
        parser_id: Some(parser_id.to_string()),
        parser_version: Some(ANALYSIS_EXTRACTOR_VERSION.to_string()),
        confidence: Some(0.85),
        source_attribution: None,
        attrs,
    }
}

pub(crate) fn base_attrs(candidate: &EvidenceCandidate) -> BTreeMap<String, Value> {
    let mut attrs = BTreeMap::new();
    attrs.insert(
        "dataSourceId".to_string(),
        Value::String(candidate.data_source_id.clone()),
    );
    attrs.insert(
        "sourcePath".to_string(),
        Value::String(candidate.path.clone()),
    );
    attrs
}

pub(crate) fn browser_attrs(
    candidate: &EvidenceCandidate,
    browser: &str,
    profile: &str,
) -> BTreeMap<String, Value> {
    let mut attrs = base_attrs(candidate);
    attrs.insert("browser".to_string(), Value::String(browser.to_string()));
    attrs.insert("profile".to_string(), Value::String(profile.to_string()));
    attrs
}

pub(crate) fn string_array_value(values: &[String]) -> Value {
    Value::Array(values.iter().cloned().map(Value::String).collect())
}

pub(crate) fn title_or_url(title: &str, url: &str) -> String {
    if title.trim().is_empty() {
        url.to_string()
    } else {
        title.to_string()
    }
}
