use persistence_sqlite::{
    open_in_memory,
    repositories::storage_object_repo::{StorageObjectRecord, StorageObjectRepo},
    runner,
};

#[test]
fn storage_objects_are_case_bound_and_relations_are_typed() {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name) VALUES ('case-storage', 'storage')",
        [],
    )
    .unwrap();
    let repo = StorageObjectRepo::new(&conn);
    repo.insert_if_absent(&StorageObjectRecord {
        id: "storage:ceph:1".to_string(),
        case_id: "case-storage".to_string(),
        object_kind: "ceph_cluster".to_string(),
        name: "Ceph".to_string(),
        identity_state: "candidate".to_string(),
        status: "ready".to_string(),
        provenance_json: "{}".to_string(),
    })
    .unwrap();
    repo.insert_if_absent(&StorageObjectRecord {
        id: "storage:rbd:1".to_string(),
        case_id: "case-storage".to_string(),
        object_kind: "ceph_rbd".to_string(),
        name: "vm-100-disk-0".to_string(),
        identity_state: "candidate".to_string(),
        status: "ready".to_string(),
        provenance_json: "{}".to_string(),
    })
    .unwrap();
    repo.insert_relation(
        "storage:ceph:1",
        "storage:rbd:1",
        "contains",
        "candidate",
        "{\"basis\":\"rbd-catalog\"}",
    )
    .unwrap();
    assert_eq!(
        repo.find("storage:rbd:1").unwrap().unwrap().object_kind,
        "ceph_rbd"
    );
}
