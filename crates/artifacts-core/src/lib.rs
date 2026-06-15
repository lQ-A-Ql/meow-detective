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

pub trait ArtifactExtractor: Send + Sync {
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
        extractor_id: None,
        extractor_version: None,
        confidence: None,
        source_attribution: None,
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
        parser_id: None,
        parser_version: None,
        confidence: None,
        source_attribution: None,
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

    #[test]
    fn test_vec_sink_collects_artifacts() {
        let mut sink = VecSink::new();
        let source_id = FileEntryId("f1".to_string());

        let a1 = new_artifact(
            "LNK",
            "Link File".to_string(),
            "A shortcut file".to_string(),
            Some(&source_id),
            BTreeMap::new(),
        );
        let a2 = new_artifact(
            "PREFETCH",
            "Prefetch Entry".to_string(),
            "Application prefetch".to_string(),
            Some(&source_id),
            BTreeMap::new(),
        );

        sink.write_artifact(a1);
        sink.write_artifact(a2);

        assert_eq!(sink.artifacts.len(), 2);
        assert_eq!(sink.artifacts[0].family, "LNK");
        assert_eq!(sink.artifacts[0].title, "Link File");
        assert!(sink.artifacts[0].source_object_id.is_some());
        assert_eq!(sink.artifacts[1].family, "PREFETCH");
        assert_eq!(sink.artifacts[1].title, "Prefetch Entry");

        // Timeline events still empty
        assert!(sink.timeline_events.is_empty());
    }

    #[test]
    fn test_vec_sink_collects_timeline_events() {
        let mut sink = VecSink::new();
        let source_id = FileEntryId("f1".to_string());
        let ts1 = Utc.with_ymd_and_hms(2024, 6, 15, 10, 30, 0).unwrap();
        let ts2 = Utc.with_ymd_and_hms(2024, 6, 15, 11, 0, 0).unwrap();

        let e1 = new_timeline_event(
            &source_id,
            "FILE_ACCESSED",
            ts1,
            "File accessed".to_string(),
            "test.txt was accessed".to_string(),
            BTreeMap::new(),
        );
        let e2 = new_timeline_event(
            &source_id,
            "FILE_MODIFIED",
            ts2,
            "File modified".to_string(),
            "test.txt was modified".to_string(),
            BTreeMap::new(),
        );

        sink.write_timeline_event(e1);
        sink.write_timeline_event(e2);

        assert_eq!(sink.timeline_events.len(), 2);
        assert_eq!(sink.timeline_events[0].event_type, "FILE_ACCESSED");
        assert_eq!(sink.timeline_events[0].title, "File accessed");
        assert_eq!(sink.timeline_events[1].event_type, "FILE_MODIFIED");
        assert_eq!(sink.timeline_events[1].source_object_id, "f1");

        // Artifacts still empty
        assert!(sink.artifacts.is_empty());
    }

    #[test]
    fn test_artifact_context_construction() {
        let file_id = FileEntryId("ctx-file-1".to_string());
        let file_path = "/test/sample.txt".to_string();
        let data = b"sample binary data".to_vec();
        let reader = Box::new(io::Cursor::new(data));

        let ctx = ArtifactContext {
            file_id: file_id.clone(),
            file_path: file_path.clone(),
            reader,
        };

        assert_eq!(ctx.file_id, file_id);
        assert_eq!(ctx.file_path, file_path);
        // reader is a Box<dyn io::Read> — constructing it without error
        // is sufficient to prove the context holds a valid reader.
    }

    #[test]
    fn test_extractor_registry_find_by_path() {
        let mut registry = ExtractorRegistry::new();
        registry.register(Box::new(StubExtractor {
            name: "lnk",
            pattern: ".lnk",
        }));
        registry.register(Box::new(StubExtractor {
            name: "prefetch",
            pattern: ".pf",
        }));
        registry.register(Box::new(StubExtractor {
            name: "evtx",
            pattern: ".evtx",
        }));

        // .lnk file should match the LNK extractor only
        let matches = registry.find_for_path("C:\\Windows\\test.lnk");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id(), "lnk");

        // .evtx file should match the EVTX extractor only
        let matches = registry.find_for_path("C:\\Windows\\System32\\winevt\\Logs\\Security.evtx");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id(), "evtx");

        // .pf file should match the Prefetch extractor only
        let matches = registry.find_for_path("C:\\Windows\\Prefetch\\CMD.EXE-abc.pf");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id(), "prefetch");
    }

    #[test]
    fn test_extractor_registry_empty_for_unknown() {
        let mut registry = ExtractorRegistry::new();
        registry.register(Box::new(StubExtractor {
            name: "lnk",
            pattern: ".lnk",
        }));
        registry.register(Box::new(StubExtractor {
            name: "prefetch",
            pattern: ".pf",
        }));

        // Unknown file types should return no matches
        let matches = registry.find_for_path("just-a-text-file.txt");
        assert!(matches.is_empty());

        // Empty registry should also return no matches
        let empty = ExtractorRegistry::new();
        assert!(empty.find_for_path("anything.lnk").is_empty());
    }

    // ---- test helpers ----

    /// Minimal stub extractor for registry-find tests.
    struct StubExtractor {
        name: &'static str,
        pattern: &'static str,
    }

    impl ArtifactExtractor for StubExtractor {
        fn id(&self) -> &'static str {
            self.name
        }

        fn display_name(&self) -> &'static str {
            self.name
        }

        fn family(&self) -> ArtifactFamily {
            ArtifactFamily {
                name: self.name.to_string(),
                description: None,
            }
        }

        fn supports_path(&self, file_path: &str) -> bool {
            file_path.contains(self.pattern)
        }

        fn run(
            &self,
            _ctx: ArtifactContext,
            _sink: &mut dyn ArtifactSink,
        ) -> Result<ExtractorReport, String> {
            Ok(ExtractorReport {
                artifacts_found: 0,
                timeline_events: 0,
                errors: vec![],
            })
        }
    }
}
