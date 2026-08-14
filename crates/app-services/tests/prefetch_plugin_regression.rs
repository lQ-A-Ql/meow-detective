//! M3 pilot regression: the Prefetch DLL plugin (`plugins-src/prefetch`)
//! is driven across the real DLL boundary against the synthetic Prefetch
//! fixture and the expected-JSON contract
//! (`testdata/artifacts/windows/prefetch/expected.json`), and its output is
//! deep-compared against the built-in track-A extractor (dual-channel
//! equality).
#![cfg(windows)]

mod prefetch_plugin_util;

use app_services::plugin_loader::{load_plugins_from_dirs, PluginExtractor};
use artifacts_core::{ArtifactContext, ArtifactExtractor, ExtractorReport, VecSink};
use chrono::{DateTime, TimeZone, Utc};
use domain::FileEntryId;
use serde_json::Value;

const FILE_ID: &str = "ds:1:pf-1";
const FILE_PATH: &str = "[P0]/Windows/Prefetch/CMD.EXE-0A1B2C3D.pf";
const RUN_TIME: &str = "2026-01-02T03:04:05Z";

/// Minimal valid uncompressed Prefetch v30 sample (84-byte header + 220-byte
/// file-info section). Mirrors the builder in
/// `plugins-src/prefetch/src/tests.rs`; keep both in sync.
fn sample_prefetch_v30() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&30u32.to_le_bytes()); // format version
    data.extend_from_slice(b"SCCA"); // uncompressed signature
    data.extend_from_slice(&0u32.to_le_bytes()); // unknown
    data.extend_from_slice(&12345u32.to_le_bytes()); // executable file size
    let mut name = [0u8; 60];
    for (index, unit) in "CMD.EXE".encode_utf16().enumerate() {
        name[index * 2..index * 2 + 2].copy_from_slice(&unit.to_le_bytes());
    }
    data.extend_from_slice(&name);
    data.extend_from_slice(&0x0A1B2C3Du32.to_le_bytes()); // hash
    data.extend_from_slice(&0u32.to_le_bytes()); // flags
    assert_eq!(data.len(), 84);
    let mut info = vec![0u8; 220];
    // 8 FILETIME run-time slots at offset 44; slot 0 = RUN_TIME.
    let unix = DateTime::parse_from_rfc3339(RUN_TIME)
        .expect("fixture timestamp")
        .timestamp();
    let filetime = ((unix + 11_644_473_600) as u64) * 10_000_000;
    info[44..52].copy_from_slice(&filetime.to_le_bytes());
    // run_count at offset 124 (doubles as the standard-section count probe).
    info[124..128].copy_from_slice(&3u32.to_le_bytes());
    // standard-section hash probe at offset 136 must be <= total length so
    // the v30 layout resolver picks the 220-byte info section.
    info[136..140].copy_from_slice(&100u32.to_le_bytes());
    data.extend_from_slice(&info);
    data
}

fn load_prefetch_plugin(dir: &std::path::Path) -> PluginExtractor {
    let plugins = load_plugins_from_dirs(&[dir.to_path_buf()]);
    assert_eq!(plugins.len(), 1, "exactly one plugin DLL staged");
    let plugin = plugins.into_iter().next().expect("one plugin");
    assert_eq!(plugin.id(), "meow.plugin.prefetch");
    assert!(plugin.supports_path(FILE_PATH));
    plugin
}

fn run_extractor(
    extractor: &dyn ArtifactExtractor,
    data: Vec<u8>,
) -> (VecSink, Result<ExtractorReport, String>) {
    let ctx = ArtifactContext {
        file_id: FileEntryId(FILE_ID.to_string()),
        file_path: FILE_PATH.to_string(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = extractor.run(ctx, &mut sink);
    (sink, report)
}

fn run_plugin(
    plugin: &PluginExtractor,
    data: Vec<u8>,
) -> (VecSink, Result<ExtractorReport, String>) {
    run_extractor(plugin, data)
}

fn run_builtin(data: Vec<u8>) -> (VecSink, ExtractorReport) {
    let (sink, report) = run_extractor(&artifacts_windows::PrefetchExtractor, data);
    (
        sink,
        report.expect("built-in prefetch extractor must succeed"),
    )
}

/// The guaranteedFields of the expected-JSON contract are the release
/// promise: every guaranteed attrs key must be present in the plugin output
/// and hold the fixture value.
#[test]
fn plugin_output_satisfies_expected_json_contract() {
    let dir = prefetch_plugin_util::stage_prefetch_plugin();
    let plugin = load_prefetch_plugin(dir.path());
    let data = sample_prefetch_v30();
    assert_eq!(&data[4..8], b"SCCA", "contract: sccaSignaturePresent");

    let (sink, report) = run_plugin(&plugin, data);
    let report = report.expect("plugin run must succeed");
    assert_eq!(
        report.artifacts_found, 1,
        "contract: decompressedPayloadValid"
    );
    assert_eq!(report.errors, Vec::<String>::new());

    let expected = expected_json();
    let guaranteed = expected["expected"]["guaranteedFields"]
        .as_array()
        .expect("guaranteedFields array");
    let artifact = &sink.artifacts[0];
    for field in guaranteed {
        let name = field["name"].as_str().expect("field name");
        if name == "artifact_type" {
            // Maps to the artifact family, not an attrs key.
            assert_eq!(artifact.family, "Prefetch");
            continue;
        }
        assert!(
            artifact.attrs.contains_key(name),
            "guaranteed field '{name}' missing from plugin attrs: {:?}",
            artifact.attrs
        );
    }

    let attrs = &artifact.attrs;
    assert_eq!(
        attrs["executable"], "CMD.EXE",
        "contract: executableNotEmpty"
    );
    assert_eq!(attrs["run_count"], 3, "contract: runCountGt 0");
    assert_eq!(attrs["format_version"], 30, "contract: formatVersionInSet");
    assert_eq!(attrs["file_size"], 12345, "contract: fileSizeGt 0");
    let hash = attrs["hash"].as_str().expect("hash string");
    assert_eq!(hash, "0A1B2C3D");
    assert!(
        hash.len() == 8
            && hash
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()),
        "contract: hashIs8HexDigits"
    );
    let run_times = attrs["last_run_times"].as_array().expect("run times array");
    assert!(run_times.len() <= 8, "contract: lastRunTimesCountLe8");
    assert_eq!(run_times.len(), 1);

    assert_eq!(sink.timeline_events.len(), 1);
    let event = &sink.timeline_events[0];
    assert_eq!(event.event_type, "PROGRAM_EXECUTION");
    assert_eq!(
        event.timestamp,
        Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap()
    );

    // Provenance is host-enforced even for plugin output.
    assert_eq!(
        artifact.source_object_id.as_ref().map(|id| id.0.as_str()),
        Some(FILE_ID)
    );
    assert_eq!(
        artifact.extractor_id.as_deref(),
        Some("meow.plugin.prefetch")
    );
    assert_eq!(artifact.extractor_version.as_deref(), Some("0.1.0"));
}

fn expected_json() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/artifacts/windows/prefetch/expected.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let parsed: Value = serde_json::from_str(&text).expect("expected.json is valid JSON");
    parsed[0].clone()
}

/// Dual-channel consistency: the same fixture through the built-in
/// extractor (in-process) and the DLL plugin must yield deep-equal artifact
/// attrs (and equal titles/summaries/family/confidence).
#[test]
fn dual_channel_artifacts_are_deep_equal() {
    let dir = prefetch_plugin_util::stage_prefetch_plugin();
    let plugin = load_prefetch_plugin(dir.path());

    let (builtin_sink, builtin_report) = run_builtin(sample_prefetch_v30());
    let (plugin_sink, plugin_report) = run_plugin(&plugin, sample_prefetch_v30());
    let plugin_report = plugin_report.expect("plugin run must succeed");

    assert_eq!(
        builtin_report.artifacts_found,
        plugin_report.artifacts_found
    );
    assert_eq!(builtin_report.errors, plugin_report.errors);
    assert_eq!(builtin_sink.artifacts.len(), plugin_sink.artifacts.len());

    for (builtin, plugin) in builtin_sink.artifacts.iter().zip(&plugin_sink.artifacts) {
        assert_eq!(builtin.family, plugin.family);
        assert_eq!(builtin.title, plugin.title);
        assert_eq!(builtin.summary, plugin.summary);
        assert_eq!(builtin.confidence, plugin.confidence);
        assert_eq!(
            builtin.attrs, plugin.attrs,
            "artifact attrs must be deep-equal across channels"
        );
    }

    assert_eq!(
        builtin_sink.timeline_events.len(),
        plugin_sink.timeline_events.len()
    );
    for (builtin, plugin) in builtin_sink
        .timeline_events
        .iter()
        .zip(&plugin_sink.timeline_events)
    {
        assert_eq!(builtin.event_type, plugin.event_type);
        assert_eq!(builtin.timestamp, plugin.timestamp);
        assert_eq!(builtin.description, plugin.description);
        assert_eq!(builtin.attrs, plugin.attrs);
        // event.title is not compared: the plugin payload schema has no
        // title field, so the host derives it from event_type (design doc
        // §4); the built-in channel uses a per-event title instead.
    }
}

/// Corrupted input across the DLL boundary must come back as a typed error
/// (ParseError surfaced by the host as `Err`), never as a process abort, and
/// the same plugin instance must keep working afterwards.
#[test]
fn corrupted_input_fails_closed_then_plugin_recovers() {
    let dir = prefetch_plugin_util::stage_prefetch_plugin();
    let plugin = load_prefetch_plugin(dir.path());

    let mut truncated = 30u32.to_le_bytes().to_vec();
    truncated.extend_from_slice(b"SCCA"); // signature present, header cut off
    let (sink, report) = run_plugin(&plugin, truncated);
    let error = match report {
        Ok(_) => panic!("truncated input must surface a typed error"),
        Err(error) => error,
    };
    assert!(
        error.contains("ParseError"),
        "expected ParseError, got: {error}"
    );
    assert!(error.contains("truncated"), "parser detail kept: {error}");
    assert!(sink.artifacts.is_empty());

    // No abort, no poisoning: the same plugin still parses a valid file.
    let (sink, report) = run_plugin(&plugin, sample_prefetch_v30());
    assert_eq!(report.expect("plugin still works").artifacts_found, 1);
    assert_eq!(sink.artifacts[0].attrs["executable"], "CMD.EXE");
}

/// Registry-level plugin priority: with the plugin on the search path the
/// Prefetch family override suppresses the built-in track-A extractor for
/// `.pf` paths (fcc30a0e dedup rule), and removing the directory falls back
/// to the built-in alone.
#[test]
fn plugin_priority_dedup_suppresses_builtin_for_pf() {
    use artifacts_core::ExtractorRegistry;

    let dir = prefetch_plugin_util::stage_prefetch_plugin();
    let plugin = load_prefetch_plugin(dir.path());

    let mut registry = ExtractorRegistry::new();
    registry.register(Box::new(artifacts_windows::PrefetchExtractor));
    let families = plugin.declared_families().to_vec();
    registry.register_plugin(Box::new(plugin), families);

    let matched = registry.find_for_path(FILE_PATH);
    assert_eq!(matched.len(), 1, "plugin wins over the built-in");
    assert_eq!(matched[0].id(), "meow.plugin.prefetch");

    let mut fallback = ExtractorRegistry::new();
    fallback.register(Box::new(artifacts_windows::PrefetchExtractor));
    let matched = fallback.find_for_path(FILE_PATH);
    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].id(), "prefetch");
}
