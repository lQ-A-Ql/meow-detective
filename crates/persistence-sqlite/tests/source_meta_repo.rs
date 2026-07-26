use persistence_sqlite::{
    open_in_memory,
    repositories::source_meta_repo::{SourceMetaRepo, ARTIFACT_CURSOR_REVISION_KEY},
    runner, DbError,
};
use rusqlite::params;

fn source_connection() -> rusqlite::Connection {
    let connection = open_in_memory().expect("open source database");
    runner::run_source_all(&connection).expect("run source migrations");
    connection
}

#[test]
fn read_round_trips_inserted_source_metadata() {
    let connection = source_connection();
    connection
        .execute(
            "INSERT INTO source_meta (key, value) VALUES (?1, ?2)",
            params!["catalogState", "complete"],
        )
        .expect("insert source metadata");

    let repository = SourceMetaRepo::new(&connection);

    assert_eq!(
        repository
            .read("catalogState")
            .expect("read source metadata"),
        Some("complete".to_string())
    );
}

#[test]
fn read_returns_none_for_missing_key() {
    let connection = source_connection();
    let repository = SourceMetaRepo::new(&connection);

    assert_eq!(
        repository
            .read("does-not-exist")
            .expect("read missing metadata"),
        None
    );
}

#[test]
fn read_rejects_empty_and_overlong_keys() {
    let connection = source_connection();
    let repository = SourceMetaRepo::new(&connection);

    assert!(matches!(repository.read(""), Err(DbError::System(_))));
    assert!(matches!(
        repository.read(&"k".repeat(257)),
        Err(DbError::System(_))
    ));
}

#[test]
fn read_rejects_values_over_the_persistence_limit() {
    let connection = source_connection();
    connection
        .execute(
            "INSERT INTO source_meta (key, value) VALUES (?1, ?2)",
            params!["oversized", "v".repeat(16 * 1024 * 1024 + 1)],
        )
        .expect("insert oversized source metadata");

    let repository = SourceMetaRepo::new(&connection);

    assert!(matches!(
        repository.read("oversized"),
        Err(DbError::System(_))
    ));
}

#[test]
fn revision_read_and_bump_round_trip() {
    let connection = source_connection();
    let repository = SourceMetaRepo::new(&connection);

    assert_eq!(
        repository
            .read_revision(ARTIFACT_CURSOR_REVISION_KEY)
            .expect("read initial revision"),
        0
    );
    assert_eq!(
        repository
            .bump_revision(ARTIFACT_CURSOR_REVISION_KEY)
            .expect("create revision"),
        1
    );
    assert_eq!(
        repository
            .bump_revision(ARTIFACT_CURSOR_REVISION_KEY)
            .expect("increment revision"),
        2
    );
    assert_eq!(
        repository
            .read_revision(ARTIFACT_CURSOR_REVISION_KEY)
            .expect("read incremented revision"),
        2
    );
}

#[test]
fn revision_rejects_malformed_and_overflowing_values() {
    let connection = source_connection();
    connection
        .execute(
            "INSERT INTO source_meta (key, value) VALUES (?1, 'not-a-number')",
            [ARTIFACT_CURSOR_REVISION_KEY],
        )
        .expect("insert malformed revision");
    let repository = SourceMetaRepo::new(&connection);

    assert!(matches!(
        repository.read_revision(ARTIFACT_CURSOR_REVISION_KEY),
        Err(DbError::System(_))
    ));
    assert!(matches!(
        repository.bump_revision(ARTIFACT_CURSOR_REVISION_KEY),
        Err(DbError::System(_))
    ));

    connection
        .execute(
            "UPDATE source_meta SET value = ?1 WHERE key = ?2",
            params![u64::MAX.to_string(), ARTIFACT_CURSOR_REVISION_KEY],
        )
        .expect("insert maximum revision");
    assert!(matches!(
        repository.bump_revision(ARTIFACT_CURSOR_REVISION_KEY),
        Err(DbError::System(_))
    ));
}

#[test]
fn revision_operations_are_legacy_safe_without_source_meta_table() {
    let connection = open_in_memory().expect("open legacy database");
    let repository = SourceMetaRepo::new(&connection);

    assert_eq!(
        repository
            .read_revision(ARTIFACT_CURSOR_REVISION_KEY)
            .expect("read absent revision table"),
        0
    );
    assert_eq!(
        repository
            .bump_revision(ARTIFACT_CURSOR_REVISION_KEY)
            .expect("skip absent revision table"),
        0
    );
}
