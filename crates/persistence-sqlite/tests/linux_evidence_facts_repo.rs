use domain::DataSourceId;
use persistence_sqlite::repositories::linux_evidence_facts_repo::LinuxEvidenceFactsRepo;
use persistence_sqlite::{open_in_memory, runner};

#[test]
fn linux_evidence_facts_repo_replaces_and_reads_json() {
    let connection = open_in_memory().expect("open source database");
    runner::run_source_all(&connection).expect("run source migrations");
    let source = DataSourceId("source-linux-facts".to_string());
    connection
        .execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
             VALUES (?1, 'case-linux-facts', 'Linux facts', 'raw', '/evidence.raw', datetime('now'))",
            [&source.0],
        )
        .expect("register source");
    let repository = LinuxEvidenceFactsRepo::new(&connection);

    assert!(repository
        .find_json(&source)
        .expect("find missing facts")
        .is_none());
    repository
        .replace_json(&source, r#"{"hostname":"node-a"}"#)
        .expect("write facts");
    assert_eq!(
        repository.find_json(&source).expect("read facts"),
        Some(r#"{"hostname":"node-a"}"#.to_string())
    );

    repository
        .replace_json(&source, r#"{"hostname":"node-b"}"#)
        .expect("replace facts");
    assert_eq!(
        repository.find_json(&source).expect("read replaced facts"),
        Some(r#"{"hostname":"node-b"}"#.to_string())
    );
}
