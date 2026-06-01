use chrono::Utc;
use domain::{Artifact, ArtifactFamily, ArtifactId, FileEntryId, TimelineEvent, TimelineEventId};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io;
use uuid::Uuid;

pub trait ArtifactSink {
    fn write_artifact(&mut self, artifact: Artifact);
    fn write_timeline_event(&mut self, event: TimelineEvent);
}

pub struct ArtifactContext {
    pub file_id: FileEntryId,
    pub file_path: String,
    pub reader: Box<dyn io::Read>,
}

pub struct ExtractorReport {
    pub artifacts_found: u32,
    pub timeline_events: u32,
    pub errors: Vec<String>,
}

pub trait ArtifactExtractor {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn family(&self) -> ArtifactFamily;
    fn dependencies(&self) -> &'static [&'static str] {
        &[]
    }
    fn supports_path(&self, file_path: &str) -> bool;
    fn run(
        &self,
        ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String>;
}

pub struct VecSink {
    pub artifacts: Vec<Artifact>,
    pub timeline_events: Vec<TimelineEvent>,
}

impl VecSink {
    pub fn new() -> Self {
        Self {
            artifacts: Vec::new(),
            timeline_events: Vec::new(),
        }
    }
}

impl Default for VecSink {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtifactSink for VecSink {
    fn write_artifact(&mut self, artifact: Artifact) {
        self.artifacts.push(artifact);
    }
    fn write_timeline_event(&mut self, event: TimelineEvent) {
        self.timeline_events.push(event);
    }
}

pub struct ExtractorRegistry {
    extractors: Vec<Box<dyn ArtifactExtractor>>,
}

impl ExtractorRegistry {
    pub fn new() -> Self {
        Self {
            extractors: Vec::new(),
        }
    }

    pub fn register(&mut self, extractor: Box<dyn ArtifactExtractor>) {
        self.extractors.push(extractor);
    }

    pub fn find_for_path(&self, file_path: &str) -> Vec<&dyn ArtifactExtractor> {
        self.extractors
            .iter()
            .filter(|e| e.supports_path(file_path))
            .map(|e| e.as_ref())
            .collect()
    }

    pub fn families(&self) -> Vec<ArtifactFamily> {
        self.extractors.iter().map(|e| e.family()).collect()
    }

    pub fn all(&self) -> &[Box<dyn ArtifactExtractor>] {
        &self.extractors
    }
}

impl Default for ExtractorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn new_artifact(
    family: &str,
    title: String,
    summary: String,
    source_id: Option<&FileEntryId>,
    attrs: BTreeMap<String, Value>,
) -> Artifact {
    Artifact {
        id: ArtifactId(Uuid::new_v4().to_string()),
        family: family.to_string(),
        title,
        summary,
        source_object_id: source_id.cloned(),
        created_at: Utc::now(),
        attrs,
    }
}

pub fn new_timeline_event(
    source_id: &FileEntryId,
    event_type: &str,
    ts: chrono::DateTime<Utc>,
    title: String,
    description: String,
    attrs: BTreeMap<String, Value>,
) -> TimelineEvent {
    TimelineEvent {
        id: TimelineEventId(Uuid::new_v4().to_string()),
        source_object_id: source_id.0.clone(),
        event_type: event_type.to_string(),
        timestamp: ts,
        title,
        description,
        attrs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_vec_sink_new() {
        let sink = VecSink::new();
        assert!(sink.artifacts.is_empty());
        assert!(sink.timeline_events.is_empty());
    }

    #[test]
    fn test_vec_sink_default() {
        let sink = VecSink::default();
        assert!(sink.artifacts.is_empty());
    }

    #[test]
    fn test_extractor_registry_new() {
        let registry = ExtractorRegistry::new();
        assert!(registry.extractors.is_empty());
    }

    #[test]
    fn test_extractor_registry_default() {
        let registry = ExtractorRegistry::default();
        assert!(registry.extractors.is_empty());
    }

    #[test]
    fn test_new_artifact() {
        let source_id = FileEntryId("file-1".to_string());
        let mut attrs = BTreeMap::new();
        attrs.insert("key".to_string(), serde_json::json!("value"));

        let artifact = new_artifact(
            "LNK",
            "Test Artifact".to_string(),
            "Test Summary".to_string(),
            Some(&source_id),
            attrs,
        );

        assert_eq!(artifact.family, "LNK");
        assert_eq!(artifact.title, "Test Artifact");
        assert!(artifact.source_object_id.is_some());
    }

    #[test]
    fn test_new_artifact_no_source() {
        let artifact = new_artifact(
            "LNK",
            "Test".to_string(),
            "Summary".to_string(),
            None,
            BTreeMap::new(),
        );

        assert!(artifact.source_object_id.is_none());
    }

    #[test]
    fn test_new_timeline_event() {
        let source_id = FileEntryId("file-1".to_string());
        let ts = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

        let event = new_timeline_event(
            &source_id,
            "FILE_CREATED",
            ts,
            "File created".to_string(),
            "test.txt created".to_string(),
            BTreeMap::new(),
        );

        assert_eq!(event.event_type, "FILE_CREATED");
        assert_eq!(event.source_object_id, "file-1");
    }
}
