use super::*;
use chrono::Utc;
use transport::events::{EventEnvelope, EventTopic};

#[test]
fn emitted_envelope_deserializes_with_nested_camel_case_payload() {
    let envelope = EventEnvelope {
        event_id: "event-123".to_string(),
        topic: EventTopic::DataSourceImported,
        ts: Utc::now(),
        payload: serde_json::json!({
            "caseId": "case-123",
            "dataSourceId": "ds-456",
            "name": "C",
            "kind": "E01",
            "jobId": "job-789",
        }),
    };
    let json = serde_json::to_string(&envelope).unwrap();
    let parsed: EventEnvelope<DataSourceImportedPayload> = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.payload.case_id, "case-123");
    assert_eq!(parsed.payload.data_source_id, "ds-456");
}
