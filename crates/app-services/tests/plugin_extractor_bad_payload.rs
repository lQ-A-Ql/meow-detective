//! M2 bad-payload path: invalid JSON fails the extraction; artifacts whose
//! family was not declared in `families_json` are dropped with a warning.
#![cfg(windows)]

mod plugin_fixture_util;

use app_services::plugin_loader::load_plugins_from_dirs;
use artifacts_core::{ArtifactContext, ArtifactExtractor, VecSink};
use domain::FileEntryId;

fn load_single(
    name: &str,
) -> (
    tempfile::TempDir,
    app_services::plugin_loader::PluginExtractor,
) {
    let dir = plugin_fixture_util::stage_plugins(&[name]);
    let mut plugins = load_plugins_from_dirs(&[dir.path().to_path_buf()]);
    assert_eq!(plugins.len(), 1, "fixture {name} must load");
    (dir, plugins.remove(0))
}

fn ctx(path: &str) -> ArtifactContext {
    ArtifactContext {
        file_id: FileEntryId("ds:1:7".to_string()),
        file_path: path.to_string(),
        reader: Box::new(std::io::Cursor::new(vec![0u8; 8])),
    }
}

#[test]
fn invalid_json_payload_is_rejected() {
    let (_dir, plugin) = load_single("bad_json");
    let mut sink = VecSink::new();
    let error = plugin
        .run(ctx("C:/Evidence/x.bdj"), &mut sink)
        .err()
        .expect("invalid JSON must fail the extraction");
    assert!(
        error.contains("not valid JSON"),
        "error should describe the payload problem: {error}"
    );
    assert!(sink.artifacts.is_empty());
    assert!(sink.timeline_events.is_empty());
}

#[test]
fn undeclared_family_artifacts_are_dropped_with_a_warning() {
    let (_dir, plugin) = load_single("undeclared_family");
    let mut sink = VecSink::new();
    let report = plugin
        .run(ctx("C:/Evidence/x.udf"), &mut sink)
        .expect("a well-formed payload with a bad family is not a hard failure");

    assert_eq!(report.artifacts_found, 0);
    assert!(sink.artifacts.is_empty());
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("undeclared family")),
        "expected an undeclared-family warning, got {:?}",
        report.errors
    );
}
