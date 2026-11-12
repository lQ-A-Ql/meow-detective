use super::*;
use domain::FileEntryId;

fn candidate() -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: FileEntryId("system-evtx".to_string()),
        data_source_id: "ds-1".to_string(),
        partition_index: None,
        path: "Windows/System32/winevt/Logs/System.evtx".to_string(),
        size: 4096,
        encrypted: false,
        content_identity: "test:system-evtx".to_string(),
        companions: Vec::new(),
        modified_at: None,
        evidence_kind: "evtx_log".to_string(),
        parser: "evtx.structured".to_string(),
        category: "EventLogs".to_string(),
    }
}

#[test]
fn boot_event_artifact_attributes_preserve_timestamp() {
    let event = EvtxBootEvent {
        timestamp: "2026-07-22T01:02:03+00:00".to_string(),
        event_id: 13,
        record_id: Some(42),
        provider: Some("Microsoft-Windows-Kernel-General".to_string()),
        kind: EvtxBootEventKind::OperatingSystemShutdown,
        source_path: "Windows/System32/winevt/Logs/System.evtx".to_string(),
        note: "shutdown phase".to_string(),
        details: BTreeMap::new(),
    };

    let attrs = boot_event_attrs(&candidate(), &event);

    assert_eq!(attrs["timestamp"], event.timestamp);
    assert_eq!(attrs["eventKind"], "operatingSystemShutdown");
}
