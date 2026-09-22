use persistence_sqlite::{
    open_in_memory,
    repositories::provenance_assertion_repo::{ProvenanceAssertionRecord, ProvenanceAssertionRepo},
    runner,
};

#[test]
fn provenance_assertion_records_preserve_parser_and_source_identity() {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name) VALUES ('case-prov', 'provenance')",
        [],
    )
    .unwrap();
    let repo = ProvenanceAssertionRepo::new(&conn);
    repo.insert(&ProvenanceAssertionRecord {
        id: "assertion-1".to_string(),
        case_id: "case-prov".to_string(),
        subject_domain: "environment".to_string(),
        subject_id: "env:kubernetes:1".to_string(),
        relation_kind: "runs".to_string(),
        source_data_source_id: None,
        source_file_id: Some("file-kubelet".to_string()),
        confidence: "candidate".to_string(),
        basis: "kubelet configuration and static pod manifest".to_string(),
        parser: "kubernetes-path-detector".to_string(),
        parser_version: "1".to_string(),
        content_digest: Some("a".repeat(64)),
        details_json: "{}".to_string(),
    })
    .unwrap();

    let row: (String, String, String, String) = conn
        .query_row(
            "SELECT subject_domain, subject_id, parser, parser_version
             FROM provenance_assertions WHERE id = 'assertion-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        row,
        (
            "environment".to_string(),
            "env:kubernetes:1".to_string(),
            "kubernetes-path-detector".to_string(),
            "1".to_string()
        )
    );
}
