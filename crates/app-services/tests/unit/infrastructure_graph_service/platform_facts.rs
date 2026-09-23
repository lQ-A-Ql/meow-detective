use super::{extract, ConfigRow};
use domain::{CaseId, DataSourceId};

#[test]
fn extracts_pve_and_debian_versions_as_platform_facts() {
    let facts = extract(
        &CaseId("case".to_string()),
        &DataSourceId("source".to_string()),
        "host",
        &[
            ConfigRow {
                artifact_id: "a".to_string(),
                file_id: "f".to_string(),
                source_path: "/etc/pve/.version".to_string(),
                line_number: 1,
                line: "pve-manager/8.2-1".to_string(),
            },
            ConfigRow {
                artifact_id: "b".to_string(),
                file_id: "g".to_string(),
                source_path: "/etc/debian_version".to_string(),
                line_number: 1,
                line: "12.5".to_string(),
            },
        ],
    );
    assert_eq!(facts.len(), 2);
    assert!(facts
        .iter()
        .any(|fact| fact.subject == "pve" && fact.value == "pve-manager/8.2-1"));
    assert!(facts
        .iter()
        .any(|fact| fact.subject == "debian" && fact.value == "12.5"));
}
