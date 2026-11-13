use super::*;
use crate::analysis_service::extraction::state::{
    AnalysisOutputProducer, AnalysisOutputReplacement,
};
use chrono::DateTime;
use persistence_source::fjall::{ArtifactRecord, TimelineRecord};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const DATA_SOURCE_ID: &str = "source-1";

fn replacement(
    source_object_id: &str,
    producer: AnalysisOutputProducer,
) -> AnalysisOutputReplacement {
    AnalysisOutputReplacement {
        source_object_id: source_object_id.to_string(),
        producer,
    }
}

fn artifact_record(
    source_object_id: Option<&str>,
    data_source_id: &str,
    extractor_id: &str,
) -> ArtifactRecord {
    ArtifactRecord {
        id: "artifact".to_string(),
        data_source_id: data_source_id.to_string(),
        artifact_type: "TestArtifact".to_string(),
        source_object_id: source_object_id.map(str::to_string),
        content_sha256: None,
        attrs: serde_json::json!({ "extractorId": extractor_id }),
    }
}

fn timeline_record(
    source_object_id: &str,
    data_source_id: &str,
    parser_id: Option<&str>,
) -> TimelineRecord {
    TimelineRecord {
        id: "event".to_string(),
        data_source_id: data_source_id.to_string(),
        source_object_id: source_object_id.to_string(),
        event_type: "FILE_MODIFIED".to_string(),
        timestamp: DateTime::from_timestamp(0, 0).expect("valid timestamp"),
        title: "event".to_string(),
        description: "event".to_string(),
        parser_id: parser_id.map(str::to_string),
        parser_version: None,
        confidence: None,
        source_attribution: None,
        attrs: BTreeMap::new(),
    }
}

#[test]
fn replacement_set_dedupes_one_hundred_thousand_replacements_in_near_linear_time() {
    let started = Instant::now();
    let mut set = ReplacementSet::default();
    for index in 0..100_000u32 {
        set.insert(replacement(
            &format!("object-{index}"),
            AnalysisOutputProducer::Prefix("registry."),
        ));
    }
    let replacements = set.into_vec();
    let elapsed = started.elapsed();
    assert_eq!(replacements.len(), 100_000);
    assert!(
        elapsed < Duration::from_secs(60),
        "quadratic dedupe regression: {elapsed:?} for 100_000 inserts"
    );
}

#[test]
fn replacement_set_ignores_duplicates_and_keeps_first_insertion_order() {
    let prefix = AnalysisOutputProducer::Prefix("registry.");
    let plugin = || AnalysisOutputProducer::ExtractorId("plugin.one".to_string());
    let mut set = ReplacementSet::default();
    set.insert(replacement("object-a", prefix.clone()));
    set.insert(replacement("object-b", plugin()));
    set.insert(replacement("object-a", prefix.clone()));
    set.insert(replacement("object-a", plugin()));
    set.insert(replacement("object-b", plugin()));
    let replacements = set.into_vec();
    assert_eq!(replacements.len(), 3);
    assert_eq!(replacements[0].source_object_id, "object-a");
    assert_eq!(replacements[0].producer, prefix);
    assert_eq!(replacements[1].source_object_id, "object-b");
    assert_eq!(replacements[1].producer, plugin());
    assert_eq!(replacements[2].source_object_id, "object-a");
    assert_eq!(replacements[2].producer, plugin());
}

#[test]
fn replacement_index_checks_one_producer_per_record_regardless_of_replacement_count() {
    let replacements = (0..100_000u32)
        .map(|index| {
            replacement(
                &format!("object-{index}"),
                AnalysisOutputProducer::Prefix("registry."),
            )
        })
        .collect::<Vec<_>>();
    let index = ReplacementIndex::new(&replacements);

    // Each source object maps to exactly one producer, so matching a record is
    // one hash lookup plus one producer check no matter how many replacements
    // are pending. A missing source object is a single failed lookup.
    assert_eq!(index.by_source.len(), 100_000);
    assert_eq!(index.by_source.get("object-0").map(Vec::len), Some(1));
    assert_eq!(index.by_source.get("object-99999").map(Vec::len), Some(1));
    assert!(!index.by_source.contains_key("object-missing"));

    assert!(index.matches_artifact(
        &artifact_record(Some("object-0"), DATA_SOURCE_ID, "registry.system.keys"),
        DATA_SOURCE_ID,
    ));
    let mut record = artifact_record(None, DATA_SOURCE_ID, "registry.system.keys");
    for position in 0..100_000u32 {
        record.source_object_id = Some(format!("object-{position}"));
        assert!(index.matches_artifact(&record, DATA_SOURCE_ID));
    }
}

#[test]
fn replacement_index_matches_artifacts_by_prefix_or_exact_producer() {
    let replacements = [
        replacement("object-a", AnalysisOutputProducer::Prefix("registry.")),
        replacement("object-b", AnalysisOutputProducer::Prefix("browser.")),
        replacement(
            "object-c",
            AnalysisOutputProducer::ExtractorId("plugin.one".to_string()),
        ),
        replacement(
            "object-c",
            AnalysisOutputProducer::ExtractorId("plugin.two".to_string()),
        ),
    ];
    let index = ReplacementIndex::new(&replacements);

    assert!(index.matches_artifact(
        &artifact_record(Some("object-a"), DATA_SOURCE_ID, "registry.system.keys"),
        DATA_SOURCE_ID,
    ));
    assert!(!index.matches_artifact(
        &artifact_record(Some("object-a"), DATA_SOURCE_ID, "browser.history"),
        DATA_SOURCE_ID,
    ));
    assert!(index.matches_artifact(
        &artifact_record(Some("object-b"), DATA_SOURCE_ID, "browser.history"),
        DATA_SOURCE_ID,
    ));
    assert!(!index.matches_artifact(
        &artifact_record(Some("object-b"), DATA_SOURCE_ID, "registry.system.keys"),
        DATA_SOURCE_ID,
    ));
    assert!(index.matches_artifact(
        &artifact_record(Some("object-c"), DATA_SOURCE_ID, "plugin.one"),
        DATA_SOURCE_ID,
    ));
    assert!(index.matches_artifact(
        &artifact_record(Some("object-c"), DATA_SOURCE_ID, "plugin.two"),
        DATA_SOURCE_ID,
    ));
    assert!(!index.matches_artifact(
        &artifact_record(Some("object-c"), DATA_SOURCE_ID, "plugin.one.extra"),
        DATA_SOURCE_ID,
    ));
    assert!(!index.matches_artifact(
        &artifact_record(Some("object-c"), DATA_SOURCE_ID, "plugin"),
        DATA_SOURCE_ID,
    ));
}

#[test]
fn replacement_index_rejects_artifacts_from_other_data_sources() {
    let replacements = [replacement(
        "object-a",
        AnalysisOutputProducer::Prefix("registry."),
    )];
    let index = ReplacementIndex::new(&replacements);
    assert!(!index.matches_artifact(
        &artifact_record(Some("object-a"), "source-2", "registry.system.keys"),
        DATA_SOURCE_ID,
    ));
}

#[test]
fn native_replacement_matches_only_its_unsigned_legacy_family() {
    let scopes = [replacement(
        "object",
        AnalysisOutputProducer::Prefix("windows.native.activity."),
    )];
    let index = ReplacementIndex::new(&scopes);
    let mut legacy = artifact_record(Some("object"), DATA_SOURCE_ID, "");
    legacy.artifact_type = "LNK".into();
    legacy.attrs = serde_json::json!({});
    assert!(index.matches_artifact(&legacy, DATA_SOURCE_ID));
    legacy.artifact_type = "JumpList".into();
    assert!(index.matches_artifact(&legacy, DATA_SOURCE_ID));
    legacy.artifact_type = "Prefetch".into();
    assert!(!index.matches_artifact(&legacy, DATA_SOURCE_ID));
    legacy.artifact_type = "LNK".into();
    legacy.attrs = serde_json::json!({ "extractorId": "other.parser" });
    assert!(!index.matches_artifact(&legacy, DATA_SOURCE_ID));
    legacy.attrs = serde_json::json!({});
    legacy.source_object_id = Some("other-object".into());
    assert!(!index.matches_artifact(&legacy, DATA_SOURCE_ID));
}

#[test]
fn replacement_index_treats_missing_source_object_ids_as_empty() {
    let empty = [replacement("", AnalysisOutputProducer::Prefix("registry."))];
    let index = ReplacementIndex::new(&empty);
    assert!(index.matches_artifact(
        &artifact_record(None, DATA_SOURCE_ID, "registry.system.keys"),
        DATA_SOURCE_ID,
    ));

    let named = [replacement(
        "object-a",
        AnalysisOutputProducer::Prefix("registry."),
    )];
    let index = ReplacementIndex::new(&named);
    assert!(!index.matches_artifact(
        &artifact_record(None, DATA_SOURCE_ID, "registry.system.keys"),
        DATA_SOURCE_ID,
    ));
}

#[test]
fn replacement_index_matches_timeline_parser_ids_without_a_none_fallback() {
    let replacements = [
        replacement("object-a", AnalysisOutputProducer::Prefix("registry.")),
        replacement(
            "object-c",
            AnalysisOutputProducer::ExtractorId("plugin.one".to_string()),
        ),
    ];
    let index = ReplacementIndex::new(&replacements);

    assert!(index.matches_timeline(&timeline_record(
        "object-a",
        DATA_SOURCE_ID,
        Some("registry.system.keys"),
    )));
    assert!(!index.matches_timeline(&timeline_record(
        "object-a",
        DATA_SOURCE_ID,
        Some("browser.history"),
    )));
    assert!(index.matches_timeline(&timeline_record(
        "object-c",
        DATA_SOURCE_ID,
        Some("plugin.one"),
    )));
    assert!(!index.matches_timeline(&timeline_record(
        "object-c",
        DATA_SOURCE_ID,
        Some("plugin.one.extra"),
    )));
    // A missing parser id never matches, even for a prefix replacement.
    assert!(!index.matches_timeline(&timeline_record("object-a", DATA_SOURCE_ID, None)));
    // Replacement deletes ignore timeline data_source_id (pre-index behavior).
    assert!(index.matches_timeline(&timeline_record(
        "object-a",
        "source-2",
        Some("registry.system.keys"),
    )));
}
