use domain::structure::{
    NativeObservation, NativeVersion, ObservationLocation, SourceRevisionContent, SourceRevisionRef,
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
    SourceRevisionContent {
        logical_size: 4096,
        metadata_identity: Some("metadata-a".into()),
        logical_sha256: Some("a".repeat(64)),
    }
}

fn observations() -> Vec<NativeObservation> {
    vec![NativeObservation {
        native_identity: "lvm-pv:uuid".into(),
        version: NativeVersion::LvmLabel,
        location: ObservationLocation::SourceWindow {
            byte_offset: 0,
            byte_length: 2048,
        },
        parser_version: "fs-lvm/label-v1".into(),
        native_checksum_verified: true,
        details: serde_json::json!({"pvSize":4096}),
    }]
}

#[test]
fn immutable_revision_reuses_identical_facts_and_rejects_stale_reference() {
    let conn = setup();
    let repo = SourceRevisionRepo::new(&conn);
    let original = repo
        .bind("case-a", "source-a", &content(), &observations(), true)
        .unwrap();
    assert_eq!(original.revision, 1);
    assert_eq!(
        repo.bind("case-a", "source-a", &content(), &observations(), false)
            .unwrap(),
        original
    );
    assert_eq!(repo.observations(&original).unwrap(), observations());
    let mut changed = content();
    changed.logical_sha256 = Some("b".repeat(64));
    assert!(repo
        .bind("case-a", "source-a", &changed, &observations(), false)
        .is_err());
    let next = repo
        .bind("case-a", "source-a", &changed, &observations(), true)
        .unwrap();
    assert_eq!(next.revision, 2);
    assert!(repo.require_current(&original).is_err());
    repo.require_current(&next).unwrap();
    assert_eq!(repo.observations(&original).unwrap(), observations());
    assert!(conn
        .execute("UPDATE source_revisions SET content_json='{}'", [])
        .is_err());
}

#[test]
fn case_boundary_and_invalid_range_or_digest_are_rejected() {
    let conn = setup();
    let repo = SourceRevisionRepo::new(&conn);
    assert!(repo
        .bind("case-b", "source-a", &content(), &observations(), true)
        .is_err());
    assert!(repo
        .bind("case-a", "missing", &content(), &[], true)
        .is_err());
    let first = repo
        .bind("case-a", "source-a", &content(), &observations(), true)
        .unwrap();
    let mut wrong_case = first.clone();
    wrong_case.case_id = "case-b".into();
    assert!(repo.observations(&wrong_case).is_err());
    let mut invalid = observations();
    invalid[0].location = ObservationLocation::SourceWindow {
        byte_offset: u64::MAX,
        byte_length: 2048,
    };
    assert!(repo
        .bind("case-a", "source-a", &content(), &invalid, true)
        .is_err());
    let mut invalid_digest = content();
    invalid_digest.logical_sha256 = Some("x".repeat(64));
    assert!(repo
        .bind("case-a", "source-a", &invalid_digest, &[], true)
        .is_err());
    let second = repo
        .bind("case-b", "source-b", &content(), &observations(), true)
        .unwrap();
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
    let parent = repo
        .bind("case-a", "source-a", &content(), &observations(), true)
        .unwrap();
    let derived = repo
        .bind("case-a", "derived", &content(), &[], true)
        .unwrap();
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
    let updated = repo
        .bind("case-a", "source-a", &changed, &observations(), true)
        .unwrap();
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
