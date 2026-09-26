use super::*;

#[test]
fn member_summary_serializes_empty_collection_fields() {
    let dto = LinuxEvidenceSetMemberSummaryDto {
        member_index: 0,
        data_source_id: None,
        source_name: "node-1".to_string(),
        source_path: "node-1.E01".to_string(),
        source_kind: "e01".to_string(),
        import_state: "ready".to_string(),
        hash_status: "pending".to_string(),
        provenance_status: "partial".to_string(),
        hostname: None,
        operating_system: None,
        os_version: None,
        kernel_version: None,
        addresses: Vec::new(),
        roles: Vec::new(),
        services: Vec::new(),
        containers: Vec::new(),
        diagnostics: Vec::new(),
    };

    let json = serde_json::to_value(dto).expect("linux member summary serializes");
    for field in [
        "addresses",
        "roles",
        "services",
        "containers",
        "diagnostics",
    ] {
        assert_eq!(json[field], serde_json::json!([]), "field {field}");
    }
}
