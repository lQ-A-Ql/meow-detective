use persistence_sqlite::{
    open_in_memory,
    repositories::{
        environment_object_repo::{EnvironmentObjectRecord, EnvironmentObjectRepo},
        infrastructure_graph_repo::InfrastructureGraphRepo,
    },
    runner,
};

#[test]
fn infrastructure_graph_projects_environment_and_storage_domains() {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name) VALUES ('case-graph', 'graph')",
        [],
    )
    .unwrap();
    EnvironmentObjectRepo::new(&conn)
        .insert_if_absent(&EnvironmentObjectRecord {
            id: "env:os:1".to_string(),
            case_id: "case-graph".to_string(),
            object_kind: "os_instance".to_string(),
            name: "Linux".to_string(),
            identity_state: "candidate".to_string(),
            status: "ready".to_string(),
            provenance_json: "{}".to_string(),
        })
        .unwrap();
    let repo = InfrastructureGraphRepo::new(&conn);
    let nodes = repo.list_nodes("case-graph").unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].domain, "environment");
    assert!(repo.list_edges("case-graph").unwrap().is_empty());
}
