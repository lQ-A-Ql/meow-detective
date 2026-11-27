use domain::structure::{AcquisitionId, SourceRevisionContent, StructureViewAttempt};
use persistence_sqlite::repositories::{
    source_revision_repo::SourceRevisionRepo, structure_view_attempt_repo::StructureViewAttemptRepo,
};

#[test]
fn view_attempts_are_attempt_scoped_and_preserve_publication_history() {
    let conn = setup();
    let revisions = SourceRevisionRepo::new(&conn);
    let parent = revisions
        .bind(
            "case-a",
            "physical",
            &SourceRevisionContent::captured(
                AcquisitionId("acquisition".into()),
                4096,
                None,
                "a".repeat(64),
            ),
            true,
        )
        .unwrap();
    revisions
        .bind(
            "case-a",
            "derived",
            &SourceRevisionContent::derived(
                AcquisitionId("acquisition".into()),
                4096,
                "version".into(),
            ),
            true,
        )
        .unwrap();
    revisions
        .bind_inputs("case-a", "derived", std::slice::from_ref(&parent))
        .unwrap();
    let repo = StructureViewAttemptRepo::new(&conn);
    let first = attempt("attempt-1", "b".repeat(64));
    repo.bind("case-a", &first).unwrap();
    repo.bind("case-a", &first).unwrap();
    assert_eq!(repo.find("case-a", "derived").unwrap(), Some(first.clone()));
    let second = attempt("attempt-2", "c".repeat(64));
    repo.bind("case-a", &second).unwrap();
    assert_eq!(
        repo.find_for_attempt("case-a", "derived", "attempt-1")
            .unwrap(),
        Some(first)
    );
    assert_eq!(
        repo.find_for_attempt("case-a", "derived", "attempt-2")
            .unwrap(),
        Some(second.clone())
    );
    let mut changed = second.clone();
    changed.receipt = "d".repeat(64);
    assert!(repo.bind("case-a", &changed).is_err());
    assert!(repo.find("case-b", "derived").is_err());
}

fn setup() -> rusqlite::Connection {
    let conn = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    conn.execute_batch(
        "INSERT INTO cases(id,name) VALUES('case-a','A'),('case-b','B');
         INSERT INTO data_sources(id,case_id,name,kind,source_path) VALUES
         ('physical','case-a','P','raw','p.raw'),
         ('derived','case-a','D','md_raid','d.json');",
    )
    .unwrap();
    conn
}

fn attempt(attempt_id: &str, receipt: String) -> StructureViewAttempt {
    StructureViewAttempt {
        derived_source_id: "derived".into(),
        input_fingerprint: "e".repeat(64),
        attempt_id: attempt_id.into(),
        owner_id: "worker".into(),
        generation: "source.fjall".into(),
        receipt,
    }
}
