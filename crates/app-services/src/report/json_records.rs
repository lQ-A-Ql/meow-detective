use domain::{Artifact, TimelineEvent};
use serde_json::{json, Value};
use transport::dto::{ArtifactRowDto, TimelineEventDto};

use super::{source_identity, ReportError};

pub(crate) fn legacy_timeline_event(event: &TimelineEvent) -> Value {
    json!({
        "id": event.id.0,
        "sourceObjectId": event.source_object_id,
        "type": event.event_type,
        "ts": event.timestamp.to_rfc3339(),
        "title": event.title,
        "description": event.description,
        "parserId": event.parser_id,
        "parserVersion": event.parser_version,
        "confidence": event.confidence,
        "sourceAttribution": event.source_attribution,
    })
}

pub(crate) fn legacy_artifact(artifact: &Artifact) -> Value {
    json!({
        "id": artifact.id.0,
        "artifactType": artifact.family,
        "title": artifact.title,
        "summary": artifact.summary,
        "sourceObjectId": artifact.source_object_id.as_ref().map(|id| id.0.as_str()),
        "extractorId": artifact.extractor_id,
        "extractorVersion": artifact.extractor_version,
        "confidence": artifact.confidence,
        "sourceAttribution": artifact.source_attribution,
        "createdAt": artifact.created_at.to_rfc3339(),
    })
}

pub(crate) fn source_timeline_event(event: &TimelineEventDto) -> Result<Value, ReportError> {
    let data_source_id = source_identity::timeline_data_source_id(event)?;
    Ok(json!({
        "id": event.id,
        "dataSourceId": data_source_id.0,
        "sourceObjectId": event.source_object_id,
        "type": event.event_type,
        "ts": event.ts,
        "title": event.title,
        "description": event.description,
        "parserId": event.parser_id,
        "parserVersion": event.parser_version,
        "confidence": event.confidence,
        "sourceAttribution": event.source_attribution,
    }))
}

pub(crate) fn source_artifact(artifact: &ArtifactRowDto) -> Result<Value, ReportError> {
    let data_source_id = source_identity::artifact_data_source_id(artifact)?;
    Ok(json!({
        "id": artifact.id,
        "dataSourceId": data_source_id.0,
        "artifactType": artifact.artifact_type,
        "title": artifact.title,
        "summary": artifact.summary,
        "sourceObjectId": artifact.source_object_id,
        "extractorId": artifact.extractor_id,
        "extractorVersion": artifact.extractor_version,
        "confidence": artifact.confidence,
        "sourceAttribution": artifact.source_attribution,
        "createdAt": artifact.created_at,
    }))
}
