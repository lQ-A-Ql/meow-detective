use super::{extract_network_facts, ConfigRow};

#[test]
fn parser_extracts_host_dns_and_interface_facts_without_treating_them_as_observed_traffic() {
    let rows = vec![
        ConfigRow {
            artifact_id: "artifact-hosts".to_string(),
            file_id: "file-hosts".to_string(),
            source_path: "/etc/hosts".to_string(),
            line_number: 2,
            line: "192.0.2.10 node-a.example node-a".to_string(),
        },
        ConfigRow {
            artifact_id: "artifact-resolv".to_string(),
            file_id: "file-resolv".to_string(),
            source_path: "/etc/resolv.conf".to_string(),
            line_number: 1,
            line: "nameserver 192.0.2.53".to_string(),
        },
    ];
    let facts = extract_network_facts("case", "source", "host", &rows);
    assert_eq!(facts.len(), 3);
    assert!(facts.iter().all(|fact| fact.assertion_kind == "configured"));
    assert!(facts
        .iter()
        .any(|fact| fact.fact_kind == "dns_server" && fact.value == "192.0.2.53"));
}
