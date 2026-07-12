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
