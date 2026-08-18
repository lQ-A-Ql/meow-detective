//! Expected-JSON regression for canonical registry extraction.
//!
//! Reads `testdata/fixtures/public-small/registry/expected.json` and asserts
//! that `extract_registry_candidate` produces the promised artifacts and
//! warnings for the tiny synthetic hives.

use app_services::analysis_service::{extract_registry_candidate, EvidenceCandidate};
use domain::FileEntryId;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct ExpectedFile {
    file: String,
    #[allow(dead_code)]
    description: String,
    expected: Expected,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct Expected {
    baseline: String,
    assertions: Assertions,
    #[allow(dead_code)]
    guaranteedFields: Vec<serde_json::Value>,
    #[allow(dead_code)]
    bestEffortFields: Vec<serde_json::Value>,
    #[allow(dead_code)]
    notGuaranteedFields: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct Assertions {
    warningsCount: usize,
    artifactCount: usize,
    #[serde(default)]
    artifactCountByFamily: HashMap<String, usize>,
    timelineEventCount: usize,
    hasArtifactWithAttrs: serde_json::Map<String, serde_json::Value>,
}

fn expected_json_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/registry/expected.json")
}

fn load_expected() -> Vec<ExpectedFile> {
    let raw = fs::read_to_string(expected_json_path()).expect("read expected.json");
    serde_json::from_str(&raw).expect("parse expected.json")
}

#[test]
fn registry_fixture_expected_regression() {
    let expected_files = load_expected();
    assert!(
        !expected_files.is_empty(),
        "expected.json should not be empty"
    );

    let base_dir = expected_json_path().parent().unwrap().to_path_buf();

    for entry in expected_files {
        let hive_path = base_dir.join(&entry.file);
        let bytes = fs::read(&hive_path).unwrap_or_else(|_| panic!("read hive {}", entry.file));
        let candidate = EvidenceCandidate {
            file_id: FileEntryId(entry.file.clone()),
            data_source_id: "fixture-ds".to_string(),
            partition_index: None,
            path: entry.file.clone(),
            size: bytes.len() as u64,
            encrypted: false,
            content_identity: format!("test:{}", entry.file),
            companions: Vec::new(),
            modified_at: None,
            evidence_kind: "registry_hive".to_string(),
            parser: "registry.lookup".to_string(),
            category: "Registry".to_string(),
        };

        let outcome = extract_registry_candidate(&candidate, &bytes, None, None, None);

        let Assertions {
            warningsCount,
            artifactCount,
            artifactCountByFamily,
            timelineEventCount,
            hasArtifactWithAttrs,
        } = entry.expected.assertions;

        assert_eq!(
            outcome.warnings.len(),
            warningsCount,
            "{}: warning count mismatch; warnings={:?}",
            entry.file,
            outcome.warnings
        );
        assert_eq!(
            outcome.artifacts.len(),
            artifactCount,
            "{}: artifact count mismatch",
            entry.file
        );
        assert_eq!(
            outcome.timeline_events.len(),
            timelineEventCount,
            "{}: timeline event count mismatch",
            entry.file
        );

        let mut actual_counts: HashMap<String, usize> = HashMap::new();
        for art in &outcome.artifacts {
            *actual_counts.entry(art.family.clone()).or_default() += 1;
        }
        assert_eq!(
            actual_counts, artifactCountByFamily,
            "{}: artifact counts by family mismatch",
            entry.file
        );

        let attrs = serde_json::Value::Object(hasArtifactWithAttrs);
        assert!(
            outcome.artifacts.iter().any(|art| {
                let mut flat = serde_json::Map::new();
                flat.insert(
                    "family".to_string(),
                    serde_json::Value::String(art.family.clone()),
                );
                for (k, v) in &art.attrs {
                    flat.insert(k.clone(), v.clone());
                }
                artifact_matches(&serde_json::Value::Object(flat), &attrs)
            }),
            "{}: no artifact matched expected attrs {:?}",
            entry.file,
            attrs
        );

        assert!(
            !entry.expected.baseline.is_empty(),
            "{}: baseline must not be empty",
            entry.file
        );
    }
}

/// Check whether `actual` contains every key/value described in `expected`.
/// Values are compared recursively for objects; primitive equality otherwise.
fn artifact_matches(actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
    match (actual, expected) {
        (serde_json::Value::Object(actual_map), serde_json::Value::Object(expected_map)) => {
            expected_map.iter().all(|(key, expected_value)| {
                actual_map
                    .get(key)
                    .map(|actual_value| artifact_matches(actual_value, expected_value))
                    .unwrap_or(false)
            })
        }
        (a, b) => a == b,
    }
}
