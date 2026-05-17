//! Real fixture tests for artifact parsers.
//! Put real Windows artifact files in testdata/artifacts/windows/<type>/
//! and an expected.json file describing expected parsed values.
//! Run: cargo test -p artifacts-windows -- --ignored

use artifacts_core::{ArtifactContext, ArtifactExtractor, VecSink};
use artifacts_windows::{LnkExtractor, PrefetchExtractor, RecycleBinExtractor, RegistryExtractor};
use domain::FileEntryId;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Expected {
    file: String,
    expected: serde_json::Value,
}

fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/artifacts/windows")
}

fn load_expected(kind: &str) -> Vec<Expected> {
    let path = testdata_dir().join(kind).join("expected.json");
    if !path.exists() {
        return vec![];
    }
    let data = fs::read_to_string(path).unwrap_or_default();
    serde_json::from_str(&data).unwrap_or_default()
}

fn read_artifact_file(kind: &str, filename: &str) -> Vec<u8> {
    fs::read(testdata_dir().join(kind).join(filename)).unwrap_or_default()
}

#[test]
#[ignore]
fn prefetch_real_fixtures() {
    for exp in load_expected("prefetch") {
        let data = read_artifact_file("prefetch", &exp.file);
        if data.is_empty() {
            continue;
        }
        let extractor = PrefetchExtractor;
        let ctx = ArtifactContext {
            file_id: FileEntryId(format!("real-{}", exp.file)),
            file_path: exp.file.clone(),
            reader: Box::new(std::io::Cursor::new(data)),
        };
        let mut sink = VecSink::new();
        let report = extractor.run(ctx, &mut sink).unwrap();
        if let Some(exe) = exp.expected.get("executable") {
            let exe_str = exe.as_str().unwrap();
            let title = &sink.artifacts[0].title;
            assert!(
                title.to_lowercase().contains(&exe_str.to_lowercase()),
                "Expected executable '{}' in title '{}'",
                exe_str,
                title
            );
        }
        if let Some(run_count_gt) = exp.expected.get("run_count_gt") {
            let run_count = sink.artifacts[0].attrs["run_count"].as_u64().unwrap();
            assert!(run_count > run_count_gt.as_u64().unwrap());
        }
        if let Some(has_runs) = exp.expected.get("has_run_times") {
            if has_runs.as_bool().unwrap() {
                assert!(!sink.timeline_events.is_empty(), "Expected timeline events");
            }
        }
    }
}

#[test]
#[ignore]
fn lnk_real_fixtures() {
    for exp in load_expected("lnk") {
        let data = read_artifact_file("lnk", &exp.file);
        if data.is_empty() {
            continue;
        }
        let extractor = LnkExtractor;
        let ctx = ArtifactContext {
            file_id: FileEntryId(format!("real-{}", exp.file)),
            file_path: exp.file.clone(),
            reader: Box::new(std::io::Cursor::new(data)),
        };
        let mut sink = VecSink::new();
        let report = extractor.run(ctx, &mut sink).unwrap();
        if let Some(has_target) = exp.expected.get("has_target_path") {
            if has_target.as_bool().unwrap() {
                assert!(
                    sink.artifacts[0].attrs.contains_key("target_path"),
                    "Expected target_path"
                );
            }
        }
        if let Some(contains) = exp.expected.get("target_contains") {
            let tp = sink.artifacts[0].attrs["target_path"]
                .as_str()
                .unwrap_or("");
            assert!(
                tp.contains(contains.as_str().unwrap()),
                "Expected '{}' in target_path, got '{}'",
                contains.as_str().unwrap(),
                tp
            );
        }
    }
}

#[test]
#[ignore]
fn recycle_bin_real_fixtures() {
    for exp in load_expected("recycle-bin") {
        let data = read_artifact_file("recycle-bin", &exp.file);
        if data.is_empty() {
            continue;
        }
        let extractor = RecycleBinExtractor;
        let ctx = ArtifactContext {
            file_id: FileEntryId(format!("real-{}", exp.file)),
            file_path: format!("$Recycle.Bin/{}", exp.file),
            reader: Box::new(std::io::Cursor::new(data)),
        };
        let mut sink = VecSink::new();
        let _ = extractor.run(ctx, &mut sink).unwrap();
        assert!(!sink.artifacts.is_empty(), "Expected artifact");
    }
}

#[test]
#[ignore]
fn registry_real_fixtures() {
    for exp in load_expected("registry") {
        let data = read_artifact_file("registry", &exp.file);
        if data.is_empty() {
            continue;
        }
        let extractor = RegistryExtractor;
        let ctx = ArtifactContext {
            file_id: FileEntryId(format!("real-{}", exp.file)),
            file_path: format!("C:/Windows/System32/config/{}", exp.file),
            reader: Box::new(std::io::Cursor::new(data)),
        };
        let mut sink = VecSink::new();
        let _ = extractor.run(ctx, &mut sink).unwrap();
        assert!(!sink.artifacts.is_empty(), "Expected artifact");
    }
}
