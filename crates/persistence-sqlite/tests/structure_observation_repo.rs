use domain::structure::{
    AcquisitionId, NativeObservation, NativeVersion, ObservationIndex, ObservationLocation,
    SourceRevisionContent, SourceRevisionRef,
};
use persistence_sqlite::repositories::{
    source_revision_repo::SourceRevisionRepo,
    structure_observation_repo::{ObservationState, StructureObservationRepo},
};

#[test]
fn observation_index_publication_is_immutable_and_revision_bound() {
    let conn = setup();
    let revision = bind_revision(&conn, "source-a", "case-a");
    let index = observation_index(&revision, "detail-a");
    let repo = StructureObservationRepo::new(&conn);
    repo.insert("case-a", &index, ObservationState::Pending)
        .unwrap();
    assert!(repo.list_published(&revision).unwrap().is_empty());
    repo.mark_published("case-a", &index.id).unwrap();
    assert_eq!(repo.list_published(&revision).unwrap(), vec![index.clone()]);
    repo.insert("case-a", &index, ObservationState::Published)
        .unwrap();
    assert_eq!(repo.count_published(&revision).unwrap(), 1);
    assert!(conn
        .execute(
            "UPDATE structure_observations SET native_identity='changed'",
            []
        )
        .is_err());
    assert!(conn
        .execute("UPDATE structure_observations SET state='pending'", [])
        .is_err());
}

#[test]
fn observation_index_rejects_cross_case_and_missing_revisions() {
    let conn = setup();
    let revision = bind_revision(&conn, "source-a", "case-a");
    let repo = StructureObservationRepo::new(&conn);
    let mut foreign = observation_index(&revision, "detail-b");
    foreign.source.case_id = "case-b".into();
    assert!(repo
        .insert("case-a", &foreign, ObservationState::Pending)
        .is_err());
    let mut missing = observation_index(&revision, "detail-c");
    missing.source.revision = 99;
    assert!(repo
        .insert("case-a", &missing, ObservationState::Pending)
        .is_err());
    let mut invalid = observation_index(&revision, "detail-d");
    invalid.detail_sha256 = "x".repeat(64);
    assert!(repo
        .insert("case-a", &invalid, ObservationState::Pending)
        .is_err());
}

fn setup() -> rusqlite::Connection {
    let conn = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    conn.execute_batch(
        "INSERT INTO cases(id,name) VALUES('case-a','A'),('case-b','B');
         INSERT INTO data_sources(id,case_id,name,kind,source_path) VALUES
         ('source-a','case-a','A','raw','a.raw'),
         ('source-b','case-b','B','raw','b.raw');",
    )
    .unwrap();
    conn
}

fn bind_revision(conn: &rusqlite::Connection, source_id: &str, case_id: &str) -> SourceRevisionRef {
    SourceRevisionRepo::new(conn)
        .bind(
            case_id,
            source_id,
            &SourceRevisionContent::captured(
                AcquisitionId("acquisition-a".into()),
                4096,
                None,
                "a".repeat(64),
            ),
            true,
        )
        .unwrap()
}

fn observation_index(source: &SourceRevisionRef, digest_seed: &str) -> ObservationIndex {
    let observation = NativeObservation {
        native_identity: "lvm-pv:uuid".into(),
        version: NativeVersion::LvmLabel,
        location: ObservationLocation::SourceWindow {
            byte_offset: 0,
            byte_length: 2048,
        },
        parser_version: "fs-lvm/label-v1".into(),
        native_checksum_verified: true,
        details: serde_json::json!({"pvSize":4096}),
    };
    ObservationIndex::new(
        source.clone(),
        &observation,
        digest(digest_seed),
        "attempt-1".into(),
        "source.fjall".into(),
        "e".repeat(64),
    )
    .unwrap()
}

fn digest(seed: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(seed.as_bytes()))
}
