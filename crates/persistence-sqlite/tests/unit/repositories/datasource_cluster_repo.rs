use super::*;

fn setup_db() -> Connection {
    let conn = crate::connection::open_in_memory().unwrap();
    crate::migrations::runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at)
         VALUES ('case-1', 'case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn
}

#[test]
fn cluster_record_round_trips_and_updates_state() {
    let conn = setup_db();
    let repo = DataSourceClusterRepo::new(&conn);
    let record = DataSourceClusterRecord {
        id: "cluster-1".to_string(),
        case_id: CaseId("case-1".to_string()),
        name: "pve".to_string(),
        root_path: "D:/cluster".to_string(),
        platform: "linux".to_string(),
        profile: Some("pve".to_string()),
        manifest_rel_path: "clusters/cluster-1/cluster-manifest.json".to_string(),
        import_state: "pending".to_string(),
        member_count: 2,
        ready_count: 0,
        failed_count: 0,
        last_error: None,
    };

    repo.insert_pending(&record).unwrap();
    repo.update_state("cluster-1", "ready", 2, 0, None).unwrap();

    let stored = repo.find_by_id("cluster-1").unwrap().unwrap();
    assert_eq!(stored.id, "cluster-1");
    assert_eq!(stored.import_state, "ready");
    assert_eq!(stored.ready_count, 2);
    assert_eq!(stored.member_count, 2);
}

#[test]
fn cluster_state_update_requires_existing_cluster() {
    let conn = setup_db();
    let repo = DataSourceClusterRepo::new(&conn);

    let error = repo.update_state("missing-cluster", "failed", 0, 1, Some("failed"));

    assert!(error.is_err());
}

#[test]
fn cluster_state_rejects_invalid_state() {
    let conn = setup_db();
    let repo = DataSourceClusterRepo::new(&conn);
    let record = DataSourceClusterRecord {
        id: "cluster-1".to_string(),
        case_id: CaseId("case-1".to_string()),
        name: "pve".to_string(),
        root_path: "D:/cluster".to_string(),
        platform: "linux".to_string(),
        profile: Some("pve".to_string()),
        manifest_rel_path: "clusters/cluster-1/cluster-manifest.json".to_string(),
        import_state: "pending".to_string(),
        member_count: 2,
        ready_count: 0,
        failed_count: 0,
        last_error: None,
    };
    repo.insert_pending(&record).unwrap();

    let error = repo.update_state("cluster-1", "unknown", 0, 0, None);

    assert!(error.is_err());
}
