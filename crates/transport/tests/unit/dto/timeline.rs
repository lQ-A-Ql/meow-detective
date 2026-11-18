use super::*;

#[test]
fn timeline_event_dto_serializes_optional_provenance_as_camel_case() {
    let dto = TimelineEventDto {
        id: "timeline-1".to_string(),
        data_source_id: None,
        source_object_id: "file-1".to_string(),
        event_type: "FILE_MODIFIED".to_string(),
        ts: "2026-06-04T00:00:00Z".to_string(),
        title: "Modified".to_string(),
        description: "description".to_string(),
        parser_id: Some("timeline.macb".to_string()),
        parser_version: Some("1.0.0".to_string()),
        confidence: Some(0.8),
        source_attribution: Some("$STANDARD_INFORMATION".to_string()),
        attrs: BTreeMap::new(),
    };

    let value = serde_json::to_value(dto).unwrap();

    assert_eq!(value["sourceObjectId"], "file-1");
    assert_eq!(value["parserId"], "timeline.macb");
    assert_eq!(value["parserVersion"], "1.0.0");
    assert!((value["confidence"].as_f64().unwrap() - 0.8).abs() < 0.000001);
    assert_eq!(value["sourceAttribution"], "$STANDARD_INFORMATION");
    assert!(value.get("parser_id").is_none());
    assert!(value.get("source_attribution").is_none());
}

#[test]
fn timeline_event_dto_skips_missing_optional_provenance() {
    let dto = TimelineEventDto {
        id: "timeline-1".to_string(),
        data_source_id: None,
        source_object_id: "file-1".to_string(),
        event_type: "FILE_MODIFIED".to_string(),
        ts: "2026-06-04T00:00:00Z".to_string(),
        title: "Modified".to_string(),
        description: "description".to_string(),
        parser_id: None,
        parser_version: None,
        confidence: None,
        source_attribution: None,
        attrs: BTreeMap::new(),
    };

    let value = serde_json::to_value(dto).unwrap();

    assert!(value.get("parserId").is_none());
    assert!(value.get("parserVersion").is_none());
    assert!(value.get("confidence").is_none());
    assert!(value.get("sourceAttribution").is_none());
}

#[test]
fn timeline_facets_serialize_global_source_and_histogram_facts() {
    let dto = TimelineFacetsDto {
        total_events: 2,
        start_ts: Some("2026-06-04T00:00:00Z".to_string()),
        end_ts: Some("2026-06-04T01:00:00Z".to_string()),
        event_types: vec![TimelineFacetCountDto {
            value: "FILE_MODIFIED".to_string(),
            count: 2,
        }],
        data_sources: vec![TimelineFacetCountDto {
            value: "source-1".to_string(),
            count: 2,
        }],
        histogram: vec![TimelineHistogramBucketDto {
            start_ts: "2026-06-04T00:00:00Z".to_string(),
            end_ts: "2026-06-04T01:00:00Z".to_string(),
            count: 2,
        }],
    };

    let value = serde_json::to_value(dto).unwrap();

    assert_eq!(value["totalEvents"], 2);
    assert_eq!(value["eventTypes"][0]["value"], "FILE_MODIFIED");
    assert_eq!(value["dataSources"][0]["value"], "source-1");
    assert_eq!(value["histogram"][0]["startTs"], "2026-06-04T00:00:00Z");
}

#[test]
fn timeline_projection_status_dto_serializes_camel_case_and_skips_options() {
    let dto = TimelineProjectionStatusDto {
        state: TimelineProjectionStateDto::Deferred,
        sources: vec![TimelineProjectionSourceStatusDto {
            data_source_id: "ds-1".to_string(),
            state: TimelineProjectionStateDto::Building,
            identity: Some("gen-1".to_string()),
            inserted_events: 7,
            required_bytes: Some(1024),
            available_bytes: Some(512),
            reason: Some("busy".to_string()),
            updated_at: Some("2026-11-18T00:00:00Z".to_string()),
        }],
    };

    let value = serde_json::to_value(&dto).unwrap();
    assert_eq!(value["state"], "deferred");
    let source = &value["sources"][0];
    assert_eq!(source["dataSourceId"], "ds-1");
    assert_eq!(source["state"], "building");
    assert_eq!(source["identity"], "gen-1");
    assert_eq!(source["insertedEvents"], 7);
    assert_eq!(source["requiredBytes"], 1024);
    assert_eq!(source["availableBytes"], 512);
    assert_eq!(source["reason"], "busy");
    assert!(source.get("data_source_id").is_none());

    let round: TimelineProjectionStatusDto = serde_json::from_value(value).unwrap();
    assert_eq!(round.state, TimelineProjectionStateDto::Deferred);
    assert_eq!(round.sources[0].inserted_events, 7);
}

#[test]
fn timeline_projection_source_status_dto_skips_missing_optional_fields() {
    let dto = TimelineProjectionSourceStatusDto {
        data_source_id: "ds-2".to_string(),
        state: TimelineProjectionStateDto::Complete,
        identity: None,
        inserted_events: 0,
        required_bytes: None,
        available_bytes: None,
        reason: None,
        updated_at: None,
    };

    let value = serde_json::to_value(&dto).unwrap();
    assert!(value.get("identity").is_none());
    assert!(value.get("requiredBytes").is_none());
    assert!(value.get("availableBytes").is_none());
    assert!(value.get("reason").is_none());
    assert!(value.get("updatedAt").is_none());
    assert_eq!(value["insertedEvents"], 0);
}
