use domain::structure::{NativeVersion, SourceRevisionContent, StorageObjectVersion};
use persistence_sqlite::repositories::{
    source_revision_repo::SourceRevisionRepo,
    storage_object_repo::{StorageObjectRecord, StorageObjectRepo},
};

#[test]
fn md_version_binds_native_events_to_registered_parent_revision() {
    let conn = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    conn.execute_batch(
        "INSERT INTO cases(id,name) VALUES('case-a','A'),('case-b','B');
        INSERT INTO data_sources(id,case_id,name,kind,source_path) VALUES
        ('physical','case-a','P','raw','p.raw'),('derived','case-a','D','md_raid','array.json');",
    )
    .unwrap();
    let revisions = SourceRevisionRepo::new(&conn);
    let content = SourceRevisionContent {
        logical_size: 4096,
        metadata_identity: None,
        logical_sha256: Some("a".repeat(64)),
    };
    let parent = revisions
        .bind("case-a", "physical", &content, &[], true)
        .unwrap();
    revisions
        .bind_inputs("case-a", "derived", &[parent])
        .unwrap();
    let repo = StorageObjectRepo::new(&conn);
    repo.insert_if_absent(&StorageObjectRecord {
        id: "md-object".into(),
        case_id: "case-a".into(),
        object_kind: domain::StorageObjectKind::MdRaid.as_str().into(),
        name: "Native MD".into(),
        identity_state: "proven".into(),
        status: "discovered".into(),
        provenance_json: "{}".into(),
    })
    .unwrap();
    let version = StorageObjectVersion {
        id: "md-version".into(),
        storage_object_id: "md-object".into(),
        derived_source_id: "derived".into(),
        adapter_version: "fs-lvm/mdraid-v2".into(),
        mapping_sha256: "b".repeat(64),
        native_versions: vec![NativeVersion::Md {
            metadata_version: "V1_2".into(),
            events: 4,
        }],
    };
    repo.bind_version("case-a", &version).unwrap();
    repo.bind_version("case-a", &version).unwrap();
    assert_eq!(
        repo.version_for_source("case-a", "derived").unwrap(),
        Some(version.clone())
    );
    assert!(repo.version_for_source("case-b", "derived").is_err());
    let mut changed = version;
    changed.mapping_sha256 = "c".repeat(64);
    assert!(repo.bind_version("case-a", &changed).is_err());
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
    conn.execute("DELETE FROM data_sources WHERE id='derived'", [])
        .unwrap();
    assert!(repo.find("md-object").unwrap().is_none());
    conn.execute("DELETE FROM cases WHERE id='case-a'", [])
        .unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM storage_object_versions", [], |row| {
            row.get::<_, u64>(0)
        })
        .unwrap(),
        0
    );
}
