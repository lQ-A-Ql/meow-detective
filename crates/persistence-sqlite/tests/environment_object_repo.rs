use persistence_sqlite::{
    open_in_memory,
    repositories::environment_object_repo::{EnvironmentObjectRecord, EnvironmentObjectRepo},
    runner,
};

#[test]
fn environment_objects_are_case_bound_and_relations_are_separate_from_storage() {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name) VALUES ('case-env', 'env')",
        [],
    )
    .unwrap();
    let repo = EnvironmentObjectRepo::new(&conn);
    repo.insert_if_absent(&EnvironmentObjectRecord {
        id: "env:os:1".to_string(),
        case_id: "case-env".to_string(),
        object_kind: "os_instance".to_string(),
        name: "Linux".to_string(),
        identity_state: "candidate".to_string(),
        status: "ready".to_string(),
        provenance_json: "{}".to_string(),
    })
    .unwrap();
    repo.insert_if_absent(&EnvironmentObjectRecord {
        id: "env:k8s:1".to_string(),
        case_id: "case-env".to_string(),
        object_kind: "kubernetes".to_string(),
        name: "Kubernetes".to_string(),
        identity_state: "candidate".to_string(),
        status: "partial".to_string(),
        provenance_json: "{}".to_string(),
    })
    .unwrap();
    repo.insert_relation(
        "env:os:1",
        "env:k8s:1",
        "runs",
        "candidate",
        "{\"basis\":\"node-artifact\"}",
    )
    .unwrap();
    assert_eq!(
        repo.find("env:k8s:1").unwrap().unwrap().object_kind,
        "kubernetes"
    );
}
