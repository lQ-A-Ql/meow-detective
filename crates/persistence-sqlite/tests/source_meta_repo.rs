use persistence_sqlite::{
    open_in_memory, repositories::source_meta_repo::SourceMetaRepo, runner, DbError,
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
