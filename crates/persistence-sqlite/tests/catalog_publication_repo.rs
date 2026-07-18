use persistence_sqlite::repositories::{
    catalog_publication_repo::{seal_for, CatalogPublicationRepo},
    processing_phase_repo::{DataSourceProcessingPhaseRepo, ProcessingPhase, ProcessingPhaseClaim},
};

fn setup() -> rusqlite::Connection {
    let connection = persistence_sqlite::open_in_memory().expect("open database");
    persistence_sqlite::runner::run_all(&connection).expect("run migrations");
    connection
        .execute(
            "INSERT INTO cases (id, name) VALUES ('case-1', 'Catalog publication tests')",
            [],
        )
        .expect("insert case");
    connection
        .execute(
            "INSERT INTO data_sources (
                id, case_id, name, kind, source_path, storage_model,
                source_db_rel_path, platform, import_state, schema_version
             ) VALUES (
                'source-1', 'case-1', 'source', 'ceph_rbd',
                'ceph-rbd://cluster/image', 'source_db',
                'sources/source-1/source.db', 'linux', 'pending', ?1
             )",
            [persistence_sqlite::migrations::runner::latest_source_version()],
        )
        .expect("insert data source");
    connection
}

#[test]
fn publication_seal_is_claimed_and_finalized_only_for_current_attempt() {
    let connection = setup();
    let data_source_id = domain::DataSourceId("source-1".to_string());
    let input_fingerprint = "a".repeat(64);
    let claim = DataSourceProcessingPhaseRepo::new(&connection)
        .claim(
            &data_source_id,
            ProcessingPhase::Catalog,
            1,
            &input_fingerprint,
            "owner-1",
        )
        .expect("claim catalog");
    let ProcessingPhaseClaim::Acquired(attempt) = claim else {
        panic!("catalog claim was not acquired");
    };
    let path = "sources/source-1/source.db";
    let digest = "b".repeat(64);
    let publication = CatalogPublicationRepo::new(&connection)
        .prepare(
            &data_source_id,
            attempt.attempt_id.as_deref().unwrap_or_default(),
            &input_fingerprint,
            path,
            &digest,
        )
        .expect("prepare publication");
    assert_eq!(
        publication.seal,
        seal_for(
            &data_source_id.0,
            attempt.attempt_id.as_deref().unwrap_or_default(),
            &input_fingerprint,
            path,
            &digest
        )
    );
    let published = CatalogPublicationRepo::new(&connection)
        .mark_published(
            &data_source_id,
            attempt.attempt_id.as_deref().unwrap_or_default(),
            &publication.seal,
        )
        .expect("mark publication published");
    assert_eq!(published.state, "published");
    assert!(CatalogPublicationRepo::new(&connection)
        .is_published(&data_source_id, &input_fingerprint, path, &digest)
        .expect("check publication"));
}

#[test]
fn published_seal_cannot_be_replaced_by_a_different_attempt() {
    let connection = setup();
    let data_source_id = domain::DataSourceId("source-1".to_string());
    let first_fingerprint = "a".repeat(64);
    let first_claim = DataSourceProcessingPhaseRepo::new(&connection)
        .claim(
            &data_source_id,
            ProcessingPhase::Catalog,
            1,
            &first_fingerprint,
            "owner-1",
        )
        .expect("claim first catalog");
    let ProcessingPhaseClaim::Acquired(first_attempt) = first_claim else {
        panic!("first catalog claim was not acquired");
    };
    let path = "sources/source-1/source.db";
    let digest = "b".repeat(64);
    let first = CatalogPublicationRepo::new(&connection)
        .prepare(
            &data_source_id,
            first_attempt.attempt_id.as_deref().unwrap_or_default(),
            &first_fingerprint,
            path,
            &digest,
        )
        .expect("prepare first publication");
    CatalogPublicationRepo::new(&connection)
        .mark_published(
            &data_source_id,
            first_attempt.attempt_id.as_deref().unwrap_or_default(),
            &first.seal,
        )
        .expect("publish first publication");

    let same_attempt = CatalogPublicationRepo::new(&connection).prepare(
        &data_source_id,
        first_attempt.attempt_id.as_deref().unwrap_or_default(),
        &first_fingerprint,
        path,
        &digest,
    );
    assert!(same_attempt.is_err());

    let second = CatalogPublicationRepo::new(&connection).prepare(
        &data_source_id,
        "second-attempt",
        &"c".repeat(64),
        path,
        &"d".repeat(64),
    );
    assert!(second.is_err());
}
