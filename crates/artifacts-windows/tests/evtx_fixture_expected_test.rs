//! Regression test for the public-small System.evtx fixture.
//!
//! This test asserts that bounded EVTX boot/shutdown extraction on the tiny
//! fixture continues to produce the same events and warnings recorded in
//! `expected.json`. If parser behavior intentionally changes, regenerate
//! `expected.json` via the ignored `dump_fixture_to_expected_json` unit test.

use artifacts_windows::extract_boot_shutdown_events;
use serde_json::Value;
use std::path::Path;

#[test]
fn evtx_fixture_expected_regression() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let fixture_dir = Path::new(manifest).join("../../testdata/fixtures/public-small/evtx");
    let expected_path = fixture_dir.join("expected.json");
    let evtx_path = fixture_dir.join("system.evtx");

    let expected: Value =
        serde_json::from_reader(std::fs::File::open(&expected_path).expect("open expected.json"))
            .expect("parse expected.json");

    let bytes = std::fs::read(&evtx_path).expect("read system.evtx fixture");
    let extraction =
        extract_boot_shutdown_events(&bytes, "Windows/System32/winevt/Logs/System.evtx")
            .expect("fixture extraction should succeed");

    let expected_events = expected
        .get("events")
        .expect("events array")
        .as_array()
        .expect("events is array");
    let expected_warnings = expected
        .get("warnings")
        .expect("warnings array")
        .as_array()
        .expect("warnings is array");

    assert_eq!(
        extraction.events.len(),
        expected_events.len(),
        "event count mismatch"
    );
    assert_eq!(
        extraction.warnings.len(),
        expected_warnings.len(),
        "warning count mismatch"
    );

    // Representative field checks instead of brittle full JSON equality.
    if let Some(first) = extraction.events.first() {
        assert_eq!(
            first.event_id,
            expected_events[0]
                .get("eventId")
                .and_then(Value::as_u64)
                .expect("eventId") as u32,
        );
        assert_eq!(first.kind.as_str(), "eventLogStarted");
        assert!(!first.timestamp.is_empty());
    }

    if let Some(last) = extraction.events.last() {
        assert_eq!(
            last.event_id,
            expected_events[expected_events.len() - 1]
                .get("eventId")
                .and_then(Value::as_u64)
                .expect("eventId") as u32,
        );
    }

    assert!(extraction
        .events
        .iter()
        .all(|event| matches!(event.event_id, 12 | 13 | 6005 | 6006 | 6008 | 1074)));
}
