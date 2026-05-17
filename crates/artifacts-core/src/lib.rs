use domain::{Artifact, ArtifactId, ArtifactFamily, FileEntryId, TimelineEvent, TimelineEventId};
use chrono::Utc;
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
    fn dependencies(&self) -> &'static [&'static str] { &[] }
    fn supports(&self, ctx: &ArtifactContext) -> bool;
    fn run(&self, ctx: ArtifactContext, sink: &mut dyn ArtifactSink) -> Result<ExtractorReport, String>;
}

pub struct VecSink {
    pub artifacts: Vec<Artifact>,
    pub timeline_events: Vec<TimelineEvent>,
}

impl VecSink {
    pub fn new() -> Self {
        Self { artifacts: Vec::new(), timeline_events: Vec::new() }
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
        Self { extractors: Vec::new() }
    }

    pub fn register(&mut self, extractor: Box<dyn ArtifactExtractor>) {
        self.extractors.push(extractor);
    }

    pub fn find_for(&self, ctx: &ArtifactContext) -> Vec<&dyn ArtifactExtractor> {
        self.extractors.iter().filter(|e| e.supports(ctx)).map(|e| e.as_ref()).collect()
    }

    pub fn families(&self) -> Vec<ArtifactFamily> {
        self.extractors.iter().map(|e| e.family()).collect()
    }

    pub fn all(&self) -> &[Box<dyn ArtifactExtractor>] {
        &self.extractors
    }
}

pub fn new_artifact(family: &str, title: String, summary: String, source_id: Option<&FileEntryId>, attrs: BTreeMap<String, Value>) -> Artifact {
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

pub fn new_timeline_event(source_id: &FileEntryId, event_type: &str, ts: chrono::DateTime<Utc>, title: String, description: String, attrs: BTreeMap<String, Value>) -> TimelineEvent {
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
