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
    /// Families each plugin extractor supersedes on a shared path hit
    /// (plugin-priority rule: hit path × family). Parallel to `extractors`;
    /// empty for built-in extractors.
    plugin_family_overrides: Vec<Vec<String>>,
}

impl ExtractorRegistry {
    pub fn new() -> Self {
        Self {
            extractors: Vec::new(),
            plugin_family_overrides: Vec::new(),
        }
    }

    pub fn register(&mut self, extractor: Box<dyn ArtifactExtractor>) {
        self.extractors.push(extractor);
        self.plugin_family_overrides.push(Vec::new());
    }

    /// Register a plugin extractor together with its declared families. When
    /// the plugin and a built-in extractor both support a path and share a
    /// family, the built-in is skipped for that path (plugin wins).
    pub fn register_plugin(
        &mut self,
        extractor: Box<dyn ArtifactExtractor>,
        families: Vec<String>,
    ) {
        self.extractors.push(extractor);
        self.plugin_family_overrides.push(families);
    }

    pub fn find_for_path(&self, file_path: &str) -> Vec<&dyn ArtifactExtractor> {
        let matching = self
            .extractors
            .iter()
            .enumerate()
            .filter(|(_, e)| e.supports_path(file_path))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if matching
            .iter()
            .all(|index| self.plugin_family_overrides[*index].is_empty())
        {
            return matching
                .into_iter()
                .map(|index| self.extractors[index].as_ref())
                .collect();
        }
        let overridden = matching
            .iter()
            .flat_map(|index| self.plugin_family_overrides[*index].iter())
            .collect::<std::collections::HashSet<_>>();
        matching
            .into_iter()
            .filter(|index| {
                !self.plugin_family_overrides[*index].is_empty()
                    || !overridden.contains(&self.extractors[*index].family().name)
            })
            .map(|index| self.extractors[index].as_ref())
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
#[path = "../tests/unit/lib.rs"]
mod tests;
