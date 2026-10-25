use transport::dto::bolt_reader::*;

#[test]
fn bolt_protocol_round_trips_requests_reports_and_failures() {
    let request = BoltReadRequestDto {
        protocol_version: BOLT_READER_PROTOCOL_VERSION,
        request_id: "request-1".into(),
        input_path: "workspace/input.db".into(),
        input_size: 4096,
        input_sha256: "a".repeat(64),
        prefix: vec![b"v1".to_vec()],
        record_keys: vec![b"digest".to_vec()],
        value_keys: Vec::new(),
        parent_depth: Some(2),
        bucket_parent_depth: None,
        parent_suffix: Vec::new(),
        include_buckets: false,
        limits: BoltReadLimitsDto {
            max_file_bytes: 4096,
            max_entries: 10,
            max_depth: 4,
            max_output_bytes: 4096,
            memory_limit_bytes: 64 * 1024 * 1024,
            max_runtime_millis: 1000,
        },
    };
    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(value["protocolVersion"], 1);
    assert!(value.get("bucketParentDepth").is_none());
    let round_trip: BoltReadRequestDto = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(round_trip).unwrap(), value);
    for result in [
        BoltReadResultDto::Complete {
            report: BoltReadReportDto {
                page_size: 4096,
                txid: 3,
                metadata_checksum_verified: true,
                entries: vec![BoltReadEntryDto {
                    bucket_path: vec![b"key".to_vec()],
                    key: b"foo".to_vec(),
                    value_length: Some(9),
                    value: None,
                    is_bucket: false,
                }],
                visited_entries: 1,
                visited_buckets: 0,
                missing_bucket: None,
            },
        },
        BoltReadResultDto::Failed {
            code: "boltReadFailed".into(),
        },
    ] {
        let response = BoltReadResponseDto {
            protocol_version: 1,
            request_id: "request-1".into(),
            result,
        };
        let json = serde_json::to_value(response).unwrap();
        let decoded: BoltReadResponseDto = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), json);
    }
}
