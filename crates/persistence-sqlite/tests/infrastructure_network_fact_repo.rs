use persistence_sqlite::{
    open_in_memory,
    repositories::{
        environment_object_repo::{EnvironmentObjectRecord, EnvironmentObjectRepo},
        infrastructure_network_fact_repo::{
            InfrastructureNetworkFactRecord, InfrastructureNetworkFactRepo,
        },
    },
    runner,
};

fn fact(id: &str, kind: &str) -> InfrastructureNetworkFactRecord {
    InfrastructureNetworkFactRecord {
        id: id.to_string(),
        case_id: "case-network".to_string(),
        data_source_id: "source-network".to_string(),
        environment_object_id: "host-network".to_string(),
        file_id: "file-network".to_string(),
        source_path: "/etc/pve/.version".to_string(),
        line_number: 1,
        fact_kind: kind.to_string(),
        subject: "pve".to_string(),
        value: "pve-manager/8.2-1".to_string(),
        assertion_kind: "configured".to_string(),
        confidence: "candidate".to_string(),
        parser: "test".to_string(),
        source_artifact_id: "artifact-network".to_string(),
    }
}

#[test]
fn replacement_deduplicates_exact_facts_and_accepts_platform_kinds() {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name) VALUES ('case-network', 'network')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path) VALUES ('source-network', 'case-network', 'source', 'e01', 'source.E01')",
        [],
    )
    .unwrap();
    EnvironmentObjectRepo::new(&conn)
        .insert_if_absent(&EnvironmentObjectRecord {
            id: "host-network".to_string(),
            case_id: "case-network".to_string(),
            object_kind: "physical_host".to_string(),
            name: "host".to_string(),
            identity_state: "candidate".to_string(),
            status: "ready".to_string(),
            provenance_json: r#"{"dataSourceId":"source-network"}"#.to_string(),
        })
        .unwrap();
    let repo = InfrastructureNetworkFactRepo::new(&conn);
    let first = fact("fact-1", "platform_version");
    let duplicate = fact("fact-2", "platform_version");
    repo.replace_for_source("case-network", "source-network", &[first, duplicate])
        .unwrap();
    let facts = repo.list_for_case("case-network").unwrap();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].id, "fact-1");
}
