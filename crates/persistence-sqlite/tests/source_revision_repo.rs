use domain::structure::{
    AcquisitionId, DigestVerification, SourceRevisionContent, SourceRevisionRef,
};
use persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo;

fn setup() -> rusqlite::Connection {
    let conn = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    conn.execute_batch(
        "INSERT INTO cases(id,name) VALUES('case-a','A'),('case-b','B');
        INSERT INTO data_sources(id,case_id,name,kind,source_path) VALUES
        ('source-a','case-a','A','raw','a.raw'),('source-b','case-b','B','raw','b.raw');",
    )
    .unwrap();
    conn
}

fn content() -> SourceRevisionContent {
    SourceRevisionContent::captured(
        AcquisitionId("acquisition-a".into()),
        4096,
        Some("metadata-a".into()),
        "a".repeat(64),
    )
}

#[test]
fn immutable_revision_reuses_identical_facts_and_rejects_stale_reference() {
    let conn = setup();
    let repo = SourceRevisionRepo::new(&conn);
    let original = repo.bind("case-a", "source-a", &content(), true).unwrap();
    assert_eq!(original.revision, 1);
    let (_, persisted) = repo.current("case-a", "source-a").unwrap();
    assert_eq!(persisted.acquisition_id.0, "acquisition-a");
    assert_eq!(
        persisted.logical_sha256_status,
        DigestVerification::Verified
    );
    assert_eq!(
        repo.bind("case-a", "source-a", &content(), false).unwrap(),
        original
    );
    let mut changed = content();
    changed.logical_sha256 = Some("b".repeat(64));
    assert!(repo.bind("case-a", "source-a", &changed, false).is_err());
    let next = repo.bind("case-a", "source-a", &changed, true).unwrap();
    assert_eq!(next.revision, 2);
    assert!(repo.require_current(&original).is_err());
    repo.require_current(&next).unwrap();
    assert!(conn
        .execute("UPDATE source_revisions SET content_json='{}'", [])
        .is_err());
}

#[test]
fn case_boundary_and_invalid_range_or_digest_are_rejected() {
    let conn = setup();
    let repo = SourceRevisionRepo::new(&conn);
    assert!(repo.bind("case-b", "source-a", &content(), true).is_err());
    assert!(repo.bind("case-a", "missing", &content(), true).is_err());
    let first = repo.bind("case-a", "source-a", &content(), true).unwrap();
    let mut wrong_case = first.clone();
    wrong_case.case_id = "case-b".into();
    assert!(repo.require_current(&wrong_case).is_err());
    let mut invalid_digest = content();
    invalid_digest.logical_sha256 = Some("x".repeat(64));
    assert!(repo
        .bind("case-a", "source-a", &invalid_digest, true)
        .is_err());
    let mut inconsistent = content();
    inconsistent.logical_sha256_status = DigestVerification::Missing;
    assert!(repo
        .bind("case-a", "source-a", &inconsistent, true)
        .is_err());
    let second = repo.bind("case-b", "source-b", &content(), true).unwrap();
    assert_eq!(second.revision, 1);
    assert_ne!(first, second);
    assert!(repo
        .require_current(&SourceRevisionRef {
            revision: 0,
            ..first
        })
        .is_err());
}

#[test]
fn fixed_inputs_reject_rebinding_cycles_and_parent_changes() {
    let conn = setup();
    conn.execute_batch(
        "INSERT INTO data_sources(id,case_id,name,kind,source_path) VALUES
        ('derived','case-a','D','md_raid','array.json');",
    )
    .unwrap();
    let repo = SourceRevisionRepo::new(&conn);
    let parent = repo.bind("case-a", "source-a", &content(), true).unwrap();
    let derived = repo.bind("case-a", "derived", &content(), true).unwrap();
    repo.bind_inputs("case-a", "derived", std::slice::from_ref(&parent))
        .unwrap();
    repo.bind_inputs("case-a", "derived", std::slice::from_ref(&parent))
        .unwrap();
    assert_eq!(
        repo.inputs("case-a", "derived").unwrap(),
        vec![parent.clone()]
    );
    assert!(repo.bind_inputs("case-a", "source-a", &[derived]).is_err());
    assert!(conn
        .execute("DELETE FROM data_sources WHERE id='source-a'", [])
        .is_err());
    let mut changed = content();
    changed.logical_sha256 = Some("b".repeat(64));
    let updated = repo.bind("case-a", "source-a", &changed, true).unwrap();
    assert!(repo.require_current_inputs("case-a", "derived").is_err());
    assert!(repo.bind_inputs("case-a", "derived", &[updated]).is_err());
    conn.execute("DELETE FROM cases WHERE id='case-a'", [])
        .unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM structure_view_inputs", [], |row| row
            .get::<_, u64>(
            0
        ))
        .unwrap(),
        0
    );
}

fn source_revision(conn: &rusqlite::Connection, source_id: &str) -> SourceRevisionRef {
    conn.execute(
        "INSERT INTO data_sources(id,case_id,name,kind,source_path) VALUES(?1,'case-a',?1,'raw',?1)",
        [source_id],
    )
    .unwrap();
    SourceRevisionRepo::new(conn)
        .bind("case-a", source_id, &content(), true)
        .unwrap()
}

#[test]
fn stale_ancestor_invalidates_descendants_and_preserves_unrelated_branch() {
    let conn = setup();
    let repo = SourceRevisionRepo::new(&conn);
    let physical = repo.bind("case-a", "source-a", &content(), true).unwrap();
    let middle = source_revision(&conn, "middle");
    let derived = source_revision(&conn, "derived");
    let unrelated = source_revision(&conn, "unrelated");
    let other_parent = source_revision(&conn, "other-parent");
    repo.bind_inputs("case-a", "middle", &[physical]).unwrap();
    repo.bind_inputs("case-a", "derived", std::slice::from_ref(&middle))
        .unwrap();
    repo.bind_inputs("case-a", "unrelated", &[other_parent])
        .unwrap();
    repo.require_current(&derived).unwrap();
    repo.require_current_inputs("case-a", "derived").unwrap();
    let mut replacement = content();
    replacement.logical_sha256 = Some("b".repeat(64));
    repo.bind("case-a", "source-a", &replacement, true).unwrap();
    assert!(repo.require_current(&middle).is_err());
    assert!(repo.require_current(&derived).is_err());
    assert!(repo.require_current_inputs("case-a", "derived").is_err());
    repo.require_current(&unrelated).unwrap();
    repo.require_current_inputs("case-a", "unrelated").unwrap();
    let next = source_revision(&conn, "next");
    assert!(repo
        .bind_inputs("case-a", &next.source_id, &[derived])
        .is_err());
}

#[test]
fn shared_ancestor_dag_is_valid_and_mixed_revisions_are_rejected() {
    let conn = setup();
    let repo = SourceRevisionRepo::new(&conn);
    let physical = repo.bind("case-a", "source-a", &content(), true).unwrap();
    let left = source_revision(&conn, "left");
    let right = source_revision(&conn, "right");
    source_revision(&conn, "derived");
    repo.bind_inputs("case-a", "left", std::slice::from_ref(&physical))
        .unwrap();
    repo.bind_inputs("case-a", "right", std::slice::from_ref(&physical))
        .unwrap();
    repo.bind_inputs("case-a", "derived", &[left, right])
        .unwrap();
    repo.require_current_inputs("case-a", "derived").unwrap();
    let mut replacement = content();
    replacement.logical_sha256 = Some("b".repeat(64));
    let updated = repo.bind("case-a", "source-a", &replacement, true).unwrap();
    source_revision(&conn, "new-branch");
    repo.bind_inputs("case-a", "new-branch", &[updated])
        .unwrap();
    conn.execute(
        "INSERT INTO structure_view_inputs VALUES('derived','new-branch',1)",
        [],
    )
    .unwrap();
    assert!(repo.require_current_inputs("case-a", "derived").is_err());
}

#[test]
fn persisted_cycles_cross_case_and_missing_ancestors_are_rejected() {
    let conn = setup();
    let repo = SourceRevisionRepo::new(&conn);
    source_revision(&conn, "first");
    source_revision(&conn, "second");
    conn.execute_batch(
        "INSERT INTO structure_view_inputs VALUES('first','second',1),('second','first',1);",
    )
    .unwrap();
    assert!(repo.require_current_inputs("case-a", "first").is_err());
    let foreign = repo.bind("case-b", "source-b", &content(), true).unwrap();
    source_revision(&conn, "foreign-middle");
    source_revision(&conn, "foreign-root");
    conn.execute(
        "INSERT INTO structure_view_inputs VALUES('foreign-middle',?1,?2)",
        rusqlite::params![foreign.source_id, foreign.revision],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO structure_view_inputs VALUES('foreign-root','foreign-middle',1)",
        [],
    )
    .unwrap();
    assert!(repo
        .require_current_inputs("case-a", "foreign-root")
        .is_err());
    let parent = repo.bind("case-a", "source-a", &content(), true).unwrap();
    let middle = source_revision(&conn, "missing-middle");
    source_revision(&conn, "missing-root");
    repo.bind_inputs("case-a", "missing-middle", &[parent])
        .unwrap();
    repo.bind_inputs("case-a", "missing-root", &[middle])
        .unwrap();
    let transaction = conn.unchecked_transaction().unwrap();
    transaction
        .execute("DELETE FROM data_sources WHERE id='source-a'", [])
        .unwrap();
    assert!(SourceRevisionRepo::new(&transaction)
        .require_current_inputs("case-a", "missing-root")
        .is_err());
}

#[test]
fn ancestor_depth_limit_counts_longest_real_path_in_shared_dag() {
    let conn = setup();
    let transaction = conn.unchecked_transaction().unwrap();
    for index in 0..67 {
        source_revision(&transaction, &format!("node-{index:03}"));
    }
    for index in 0..66 {
        transaction
            .execute(
                "INSERT INTO structure_view_inputs VALUES(?1,?2,1)",
                rusqlite::params![format!("node-{index:03}"), format!("node-{:03}", index + 1)],
            )
            .unwrap();
    }
    transaction
        .execute(
            "INSERT INTO structure_view_inputs VALUES('node-000','node-066',1)",
            [],
        )
        .unwrap();
    let repo = SourceRevisionRepo::new(&transaction);
    repo.require_current_inputs("case-a", "node-002").unwrap();
    assert!(repo.require_current_inputs("case-a", "node-001").is_err());
    assert!(repo.require_current_inputs("case-a", "node-000").is_err());
}

#[test]
fn ancestor_graph_rejects_excess_unique_nodes_and_edges() {
    let conn = setup();
    let transaction = conn.unchecked_transaction().unwrap();
    source_revision(&transaction, "wide-root");
    for index in 0..1024 {
        let source_id = format!("wide-{index:04}");
        source_revision(&transaction, &source_id);
        transaction
            .execute(
                "INSERT INTO structure_view_inputs VALUES('wide-root',?1,1)",
                [&source_id],
            )
            .unwrap();
    }
    assert!(SourceRevisionRepo::new(&transaction)
        .require_current_inputs("case-a", "wide-root")
        .is_err());
    transaction
        .execute("DELETE FROM structure_view_inputs WHERE derived_source_id='wide-root' AND parent_source_id='wide-1023'", [])
        .unwrap();
    SourceRevisionRepo::new(&transaction)
        .require_current_inputs("case-a", "wide-root")
        .unwrap();
    source_revision(&transaction, "edge-root");
    for index in 0..64 {
        source_revision(&transaction, &format!("edge-leaf-{index:02}"));
    }
    for branch in 0..66 {
        let branch_id = format!("edge-branch-{branch:02}");
        source_revision(&transaction, &branch_id);
        transaction
            .execute(
                "INSERT INTO structure_view_inputs VALUES('edge-root',?1,1)",
                [&branch_id],
            )
            .unwrap();
        for leaf in 0..64 {
            transaction
                .execute(
                    "INSERT INTO structure_view_inputs VALUES(?1,?2,1)",
                    rusqlite::params![branch_id, format!("edge-leaf-{leaf:02}")],
                )
                .unwrap();
        }
    }
    assert!(SourceRevisionRepo::new(&transaction)
        .require_current_inputs("case-a", "edge-root")
        .is_err());
    transaction
        .execute("DELETE FROM structure_view_inputs WHERE derived_source_id='edge-root' AND parent_source_id >= 'edge-branch-62'", [])
        .unwrap();
    SourceRevisionRepo::new(&transaction)
        .require_current_inputs("case-a", "edge-root")
        .unwrap();
}
