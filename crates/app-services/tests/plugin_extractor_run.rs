//! M2 extraction path: a valid plugin payload becomes domain artifacts and
//! timeline events, with provenance fields forcibly overwritten by the host.
#![cfg(windows)]

mod plugin_fixture_util;

use app_services::plugin_loader::load_plugins_from_dirs;
use artifacts_core::{ArtifactContext, ArtifactExtractor, VecSink};
use chrono::{TimeZone, Utc};
use domain::FileEntryId;

const FILE_ID: &str = "ds:1:42";
const FILE_PATH: &str = "[P0]/Evidence/FOO.MFX";

fn run_good_plugin() -> (VecSink, artifacts_core::ExtractorReport) {
    let dir = plugin_fixture_util::stage_plugins(&["good"]);
    let plugins = load_plugins_from_dirs(&[dir.path().to_path_buf()]);
    assert_eq!(plugins.len(), 1);
    let ctx = ArtifactContext {
        file_id: FileEntryId(FILE_ID.to_string()),
        file_path: FILE_PATH.to_string(),
        reader: Box::new(std::io::Cursor::new(vec![0u8; 16])),
    };
    let mut sink = VecSink::new();
    let report = plugins[0]
        .run(ctx, &mut sink)
        .expect("fixture run must succeed");
    (sink, report)
}

#[test]
fn valid_payload_becomes_artifacts_and_timeline_events() {
    let (sink, report) = run_good_plugin();

    assert_eq!(report.artifacts_found, 1);
    assert_eq!(report.timeline_events, 1);
    assert_eq!(report.errors, vec!["fixture warning".to_string()]);

    let artifact = &sink.artifacts[0];
    assert_eq!(artifact.family, "Fixture");
    assert_eq!(artifact.title, "fixture artifact");
    assert_eq!(artifact.confidence, Some(0.9));
    assert_eq!(
        artifact.attrs.get("origin"),
        Some(&serde_json::Value::String("plugin".to_string()))
    );

    let event = &sink.timeline_events[0];
    assert_eq!(event.event_type, "Execution");
    assert_eq!(event.description, "fixture event");
    assert_eq!(
        event.timestamp,
        Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap()
    );
}

#[test]
fn host_enforces_provenance_fields() {
    let (sink, _) = run_good_plugin();
    let file_id = FileEntryId(FILE_ID.to_string());

    let artifact = &sink.artifacts[0];
    // source_object_id is forced to the requested FileEntryId (Gotcha #13).
    assert_eq!(artifact.source_object_id.as_ref(), Some(&file_id));
    assert_eq!(artifact.extractor_id.as_deref(), Some("meow.fixture.good"));
    assert_eq!(artifact.extractor_version.as_deref(), Some("0.1.0"));
    assert_eq!(artifact.source_attribution.as_deref(), Some(FILE_PATH));

    let event = &sink.timeline_events[0];
    assert_eq!(event.source_object_id, FILE_ID);
    assert_eq!(event.parser_id.as_deref(), Some("meow.fixture.good"));
    assert_eq!(event.parser_version.as_deref(), Some("0.1.0"));
    assert_eq!(event.source_attribution.as_deref(), Some(FILE_PATH));
}
