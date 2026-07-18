use super::*;

#[test]
fn data_source_summary_serializes_provenance_fields_as_frontend_contract() {
    let dto = DataSourceSummaryDto {
        id: "ds-1".to_string(),
        name: "Evidence".to_string(),
        kind: "raw".to_string(),
        source_path: "D:/evidence/disk.raw".to_string(),
        imported_at: "2026-06-04T00:00:00Z".to_string(),
        file_count: Some(42),
        storage_model: Some("source_db".to_string()),
        source_db_rel_path: Some("sources/ds-1/source.db".to_string()),
        index_rel_path: Some("sources/ds-1/index".to_string()),
        staging_rel_path: Some("staging/ds-1".to_string()),
        platform: "windows".to_string(),
        profile: Some("triage".to_string()),
        import_state: Some("ready".to_string()),
        schema_version: Some("source_002_data_source_metadata".to_string()),
        last_error: None,
        processing: Some(DataSourceProcessingSummaryDto {
            state: "ready".to_string(),
            total_count: 6,
            ready_count: 6,
            pending_count: 0,
            running_count: 0,
            failed_count: 0,
            deferred_count: 0,
            last_error: None,
            phases: vec![DataSourceProcessingPhaseDto {
                phase: "catalog".to_string(),
                state: "ready".to_string(),
                version: 2,
                stats: serde_json::json!({"recordCount": 42}),
                last_error: None,
                started_at: Some("2026-07-17 00:00:00".to_string()),
                completed_at: Some("2026-07-17 00:00:01".to_string()),
                heartbeat_at: Some("2026-07-17 00:00:01".to_string()),
                lease_expires_at: None,
                updated_at: "2026-07-17 00:00:01".to_string(),
            }],
        }),
        source_hash: Some("a".repeat(64)),
        hash_status: Some("hashed".to_string()),
        canonical_path: Some("D:/canonical/disk.raw".to_string()),
        evidence_size: Some(4096),
        reader_kind: Some("raw".to_string()),
        provenance_status: Some("recorded".to_string()),
        warnings: vec!["metadata warning".to_string()],
        partitions: vec![DataSourcePartitionDto {
            index: 1,
            name: "Basic data".to_string(),
            kind_label: "NTFS".to_string(),
            status: "supported".to_string(),
            offset: 1048576,
            length: 4096,
            type_guid: None,
            filesystem: Some("NTFS".to_string()),
            unlock_hint: None,
        }],
    };

    let value = serde_json::to_value(dto).unwrap();

    assert_eq!(value["sourcePath"], "D:/evidence/disk.raw");
    assert_eq!(value["importedAt"], "2026-06-04T00:00:00Z");
    assert_eq!(value["fileCount"], 42);
    assert_eq!(value["storageModel"], "source_db");
    assert_eq!(value["sourceDbRelPath"], "sources/ds-1/source.db");
    assert_eq!(value["indexRelPath"], "sources/ds-1/index");
    assert_eq!(value["stagingRelPath"], "staging/ds-1");
    assert_eq!(value["platform"], "windows");
    assert_eq!(value["profile"], "triage");
    assert_eq!(value["importState"], "ready");
    assert_eq!(value["schemaVersion"], "source_002_data_source_metadata");
    assert_eq!(value["processing"]["state"], "ready");
    assert_eq!(value["processing"]["readyCount"], 6);
    assert_eq!(value["processing"]["phases"][0]["phase"], "catalog");
    assert_eq!(value["processing"]["phases"][0]["version"], 2);
    assert_eq!(value["processing"]["phases"][0]["stats"]["recordCount"], 42);
    assert_eq!(value["sourceHash"], "a".repeat(64));
    assert_eq!(value["hashStatus"], "hashed");
    assert_eq!(value["canonicalPath"], "D:/canonical/disk.raw");
    assert_eq!(value["evidenceSize"], 4096);
    assert_eq!(value["readerKind"], "raw");
    assert_eq!(value["provenanceStatus"], "recorded");
    assert_eq!(value["warnings"][0], "metadata warning");
    assert_eq!(value["partitions"][0]["kindLabel"], "NTFS");
    assert!(value.get("source_hash_sha256").is_none());
    assert!(value.get("source_hash").is_none());
    assert!(value.get("canonical_source_path").is_none());
    assert!(value.get("canonical_path").is_none());
}

#[test]
fn data_source_summary_skips_missing_optional_provenance_fields() {
    let dto = DataSourceSummaryDto {
        id: "ds-legacy".to_string(),
        name: "Legacy".to_string(),
        kind: "raw".to_string(),
        source_path: "D:/legacy.raw".to_string(),
        imported_at: "2026-06-04T00:00:00Z".to_string(),
        file_count: None,
        storage_model: None,
        source_db_rel_path: None,
        index_rel_path: None,
        staging_rel_path: None,
        platform: "linux".to_string(),
        profile: None,
        import_state: None,
        schema_version: None,
        last_error: None,
        processing: None,
        source_hash: None,
        hash_status: None,
        canonical_path: None,
        evidence_size: None,
        reader_kind: None,
        provenance_status: None,
        warnings: Vec::new(),
        partitions: Vec::new(),
    };

    let value = serde_json::to_value(dto).unwrap();

    assert!(value.get("fileCount").is_none());
    assert!(value.get("storageModel").is_none());
    assert!(value.get("sourceDbRelPath").is_none());
    assert!(value.get("indexRelPath").is_none());
    assert!(value.get("stagingRelPath").is_none());
    assert_eq!(value["platform"], "linux");
    assert!(value.get("profile").is_none());
    assert!(value.get("importState").is_none());
    assert!(value.get("schemaVersion").is_none());
    assert!(value.get("lastError").is_none());
    assert!(value.get("processing").is_none());
    assert!(value.get("sourceHash").is_none());
    assert!(value.get("hashStatus").is_none());
    assert!(value.get("canonicalPath").is_none());
    assert!(value.get("evidenceSize").is_none());
    assert!(value.get("readerKind").is_none());
    assert!(value.get("provenanceStatus").is_none());
    assert!(value.get("warnings").is_none());
    assert!(value.get("partitions").is_none());
}
