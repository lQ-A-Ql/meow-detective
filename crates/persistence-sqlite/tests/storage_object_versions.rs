use domain::structure::{
    AcquisitionId, MdMetadataVersion, NativeObjectId, NativeVersion, RevisionConsistency,
    SourceRevisionContent, SourceRevisionRef, StorageObjectVersion, StorageObjectVersionKind,
};
use persistence_sqlite::repositories::{
    source_revision_repo::SourceRevisionRepo,
    storage_object_repo::{StorageObjectRecord, StorageObjectRepo},
};

#[test]
fn object_identity_stays_stable_while_versions_accumulate() {
    let conn = setup();
    let revisions = SourceRevisionRepo::new(&conn);
    let first_parent = revisions
        .bind(
            "case-a",
            "physical",
            &SourceRevisionContent::captured(
                AcquisitionId("physical".into()),
                4096,
                None,
                "a".repeat(64),
            ),
            true,
        )
        .unwrap();
    let derived_one = bind_derived(&conn, "md-one");
    revisions
        .bind_inputs("case-a", "md-one", std::slice::from_ref(&first_parent))
        .unwrap();
    let version_one = version(&derived_one, first_parent.clone(), 4, "b".repeat(64));
    let repo = StorageObjectRepo::new(&conn);
    repo.insert_if_absent(&object_record(&version_one, "case-a"))
        .unwrap();
    repo.bind_version("case-a", &version_one).unwrap();
    repo.bind_version("case-a", &version_one).unwrap();

    let second_parent = revisions
        .bind(
            "case-a",
            "physical",
            &SourceRevisionContent::captured(
                AcquisitionId("physical".into()),
                4096,
                None,
                "c".repeat(64),
            ),
            true,
        )
        .unwrap();
    let derived_two = bind_derived(&conn, "md-two");
    revisions
        .bind_inputs("case-a", "md-two", std::slice::from_ref(&second_parent))
        .unwrap();
    let version_two = version(&derived_two, second_parent, 5, "d".repeat(64));
    assert_eq!(version_one.storage_object_id, version_two.storage_object_id);
    assert_ne!(version_one.id, version_two.id);
    repo.bind_version("case-a", &version_two).unwrap();

    let versions = repo
        .versions_for_object("case-a", &version_one.storage_object_id)
        .unwrap();
    assert_eq!(versions.len(), 2);
    assert!(versions.contains(&version_one));
    assert!(versions.contains(&version_two));
    assert_eq!(
        repo.find(&version_one.storage_object_id)
            .unwrap()
            .unwrap()
            .identity_state,
        "proven"
    );
    assert_eq!(
        repo.version_for_source("case-a", "md-one").unwrap(),
        Some(version_one)
    );
    assert!(repo.version_for_source("case-b", "md-one").is_err());
}

#[test]
fn same_native_version_with_different_mapping_marks_identity_conflicted() {
    let conn = setup();
    let revisions = SourceRevisionRepo::new(&conn);
    let parent = revisions
        .bind(
            "case-a",
            "physical",
            &SourceRevisionContent::captured(
                AcquisitionId("physical".into()),
                4096,
                None,
                "a".repeat(64),
            ),
            true,
        )
        .unwrap();
    let first = bind_derived(&conn, "md-one");
    revisions
        .bind_inputs("case-a", "md-one", std::slice::from_ref(&parent))
        .unwrap();
    let version_one = version(&first, parent.clone(), 7, "b".repeat(64));
    let repo = StorageObjectRepo::new(&conn);
    repo.insert_if_absent(&object_record(&version_one, "case-a"))
        .unwrap();
    repo.bind_version("case-a", &version_one).unwrap();

    let second = bind_derived(&conn, "md-two");
    revisions
        .bind_inputs("case-a", "md-two", std::slice::from_ref(&parent))
        .unwrap();
    let version_two = version(&second, parent, 7, "c".repeat(64));
    repo.bind_version("case-a", &version_two).unwrap();

    assert_eq!(
        repo.find(&version_one.storage_object_id)
            .unwrap()
            .unwrap()
            .identity_state,
        "conflicted"
    );
    assert!(repo.bind_version("case-a", &version_two).is_ok());
}

#[test]
fn versions_are_immutable_case_scoped_and_retire_with_the_derived_source() {
    let conn = setup();
    let revisions = SourceRevisionRepo::new(&conn);
    let parent = revisions
        .bind(
            "case-b",
            "source-b",
            &SourceRevisionContent::captured(
                AcquisitionId("source-b".into()),
                4096,
                None,
                "a".repeat(64),
            ),
            true,
        )
        .unwrap();
    let derived = bind_derived(&conn, "md-one");
    revisions
        .bind_inputs("case-a", "md-one", std::slice::from_ref(&parent))
        .unwrap_err();
    let parent = revisions
        .bind(
            "case-a",
            "physical",
            &SourceRevisionContent::captured(
                AcquisitionId("physical".into()),
                4096,
                None,
                "b".repeat(64),
            ),
            true,
        )
        .unwrap();
    revisions
        .bind_inputs("case-a", "md-one", std::slice::from_ref(&parent))
        .unwrap();
    let version = version(&derived, parent, 8, "c".repeat(64));
    let repo = StorageObjectRepo::new(&conn);
    repo.insert_if_absent(&object_record(&version, "case-a"))
        .unwrap();
    repo.bind_version("case-a", &version).unwrap();
    assert!(conn
        .execute(
            "UPDATE storage_object_versions SET adapter_version='changed'",
            []
        )
        .is_err());
    assert_eq!(
        conn.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, u64>(0))
            .unwrap(),
        1
    );
    conn.execute("DELETE FROM data_sources WHERE id='md-one'", [])
        .unwrap();
    assert!(repo.find(&version.storage_object_id).unwrap().is_none());
}

fn setup() -> rusqlite::Connection {
    let conn = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    conn.execute_batch(
        "INSERT INTO cases(id,name) VALUES('case-a','A'),('case-b','B');
         INSERT INTO data_sources(id,case_id,name,kind,source_path) VALUES
         ('physical','case-a','P','raw','p.raw'),
         ('source-b','case-b','B','raw','b.raw');",
    )
    .unwrap();
    conn
}

fn bind_derived(conn: &rusqlite::Connection, source_id: &str) -> SourceRevisionRef {
    conn.execute(
        "INSERT INTO data_sources(id,case_id,name,kind,source_path)
         VALUES(?1,'case-a',?1,'md_raid',?1)",
        [source_id],
    )
    .unwrap();
    SourceRevisionRepo::new(conn)
        .bind(
            "case-a",
            source_id,
            &SourceRevisionContent::derived(
                AcquisitionId(format!("acquisition:{source_id}")),
                4096,
                format!("version:{source_id}"),
            ),
            true,
        )
        .unwrap()
}

fn version(
    derived: &SourceRevisionRef,
    parent: SourceRevisionRef,
    events: u64,
    mapping: String,
) -> StorageObjectVersion {
    StorageObjectVersion::new(
        NativeObjectId::new(
            "fs-lvm",
            "mdraid.array_uuid",
            "00112233445566778899aabbccddeeff",
            "case-a:physical",
        )
        .unwrap(),
        vec![AcquisitionId("physical".into())],
        derived.source_id.clone(),
        derived.source_id.clone(),
        "fs-lvm/mdraid-v3".into(),
        "mdraid-policy-v1".into(),
        mapping,
        vec![parent],
        vec![NativeVersion::Md {
            metadata_version: MdMetadataVersion::V1_2,
            events,
        }],
        RevisionConsistency::Coherent,
        StorageObjectVersionKind::DerivedOutput,
    )
    .unwrap()
}

fn object_record(version: &StorageObjectVersion, case_id: &str) -> StorageObjectRecord {
    StorageObjectRecord {
        id: version.storage_object_id.clone(),
        case_id: case_id.into(),
        object_kind: domain::StorageObjectKind::MdRaid.as_str().into(),
        name: "Native MD".into(),
        identity_state: "proven".into(),
        status: "discovered".into(),
        provenance_json: "{}".into(),
    }
}
