use super::*;
use artifacts_core::{ArtifactContext, ArtifactSink, ExtractorReport};
use domain::{ArtifactFamily, FileEntryId};

/// Plugin-shaped stub that emits one artifact and one timeline event.
struct GoodStub;

impl ArtifactExtractor for GoodStub {
    fn id(&self) -> &'static str {
        "meow.stub.good"
    }

    fn display_name(&self) -> &'static str {
        "Good Stub"
    }

    fn family(&self) -> ArtifactFamily {
        ArtifactFamily {
            name: "Stub".to_string(),
            description: None,
        }
    }

    fn supports_path(&self, file_path: &str) -> bool {
        file_path.to_lowercase().ends_with(".mfx")
    }

    fn run(
        &self,
        ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        let mut artifact = artifacts_core::new_artifact(
            "Stub",
            "stub title".to_string(),
            "stub summary".to_string(),
            Some(&ctx.file_id),
            Default::default(),
        );
        artifact.extractor_id = Some(self.id().to_string());
        sink.write_artifact(artifact);
        Ok(ExtractorReport {
            artifacts_found: 1,
            timeline_events: 0,
            errors: vec!["stub warning".to_string()],
        })
    }
}

/// Plugin-shaped stub whose extraction always fails.
struct FailingStub;

impl ArtifactExtractor for FailingStub {
    fn id(&self) -> &'static str {
        "meow.stub.failing"
    }

    fn display_name(&self) -> &'static str {
        "Failing Stub"
    }

    fn family(&self) -> ArtifactFamily {
        ArtifactFamily {
            name: "Stub".to_string(),
            description: None,
        }
    }

    fn supports_path(&self, file_path: &str) -> bool {
        file_path.to_lowercase().ends_with(".mfx")
    }

    fn run(
        &self,
        _ctx: ArtifactContext,
        _sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        Err("stub blew up".to_string())
    }
}

fn candidate() -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: FileEntryId("f1".to_string()),
        data_source_id: "source-1".to_string(),
        partition_index: None,
        path: "[P0]/Evidence/FOO.MFX".to_string(),
        size: 16,
        encrypted: false,
        content_identity: "test:plugin".to_string(),
        companions: Vec::new(),
        evidence_kind: "plugin".to_string(),
        parser: "meow.stub.good".to_string(),
        category: "PluginArtifacts".to_string(),
        modified_at: None,
    }
}

#[test]
fn matching_plugins_merge_outputs_and_warnings() {
    let good = GoodStub;
    let plugins: Vec<&dyn ArtifactExtractor> = vec![&good as &dyn ArtifactExtractor];
    let result = extract_plugin_candidate(&candidate(), &[0u8; 8], &[], &plugins);

    assert!(result.failures.is_empty());
    assert_eq!(result.outcome.artifacts.len(), 1);
    assert_eq!(
        result.outcome.artifacts[0].extractor_id.as_deref(),
        Some("meow.stub.good")
    );
    assert_eq!(result.outcome.warnings, vec!["stub warning".to_string()]);
}

#[test]
fn failing_plugin_degrades_to_warning_and_failure_record() {
    let good = GoodStub;
    let failing = FailingStub;
    let plugins: Vec<&dyn ArtifactExtractor> = vec![
        &failing as &dyn ArtifactExtractor,
        &good as &dyn ArtifactExtractor,
    ];
    let result = extract_plugin_candidate(&candidate(), &[0u8; 8], &[], &plugins);

    // The failing plugin never blocks the remaining plugins on the candidate.
    assert_eq!(result.outcome.artifacts.len(), 1);
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].plugin_id, "meow.stub.failing");
    assert_eq!(result.failures[0].source_path, "[P0]/Evidence/FOO.MFX");
    assert_eq!(result.failures[0].error, "stub blew up");
    assert!(result
        .outcome
        .warnings
        .iter()
        .any(|warning| warning.contains("meow.stub.failing")));
}

#[test]
fn non_matching_plugin_is_not_invoked() {
    let failing = FailingStub;
    let plugins: Vec<&dyn ArtifactExtractor> = vec![&failing as &dyn ArtifactExtractor];
    let mut txt_candidate = candidate();
    txt_candidate.path = "[P0]/Evidence/notes.txt".to_string();
    let result = extract_plugin_candidate(&txt_candidate, &[0u8; 8], &[], &plugins);

    assert!(result.outcome.artifacts.is_empty());
    assert!(result.outcome.warnings.is_empty());
    assert!(result.failures.is_empty());
}
