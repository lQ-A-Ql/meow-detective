use super::*;
use domain::{CaseId, CaseMeta};
use persistence_sqlite::repositories::case_repo::CaseRepo;
use persistence_sqlite::runner;
use rusqlite::Connection;

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    CaseRepo::new(&conn)
        .create(&CaseMeta {
            id: CaseId("case-1".to_string()),
            name: "Notebook Test Case".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .unwrap();
    conn
}

#[test]
fn create_and_retrieve_entry() {
    let conn = setup();
    let dto = create_entry(
        &conn,
        "case-1",
        "investigator",
        &NotebookEntryTypeDto::Finding,
        "Suspicious file found",
        "## Details\n\nFound `cmd.exe` with unexpected hash.",
        &["suspicious".to_string(), "cmd".to_string()],
        &NotebookEntryStatusDto::Draft,
        None,
    )
    .unwrap();

    assert_eq!(dto.case_id, "case-1");
    assert_eq!(dto.author, "investigator");
    assert_eq!(dto.entry_type, NotebookEntryTypeDto::Finding);
    assert_eq!(dto.title, "Suspicious file found");
    assert!(dto.body_markdown.contains("cmd.exe"));
    assert_eq!(dto.tags, vec!["suspicious", "cmd"]);
    assert_eq!(dto.status, NotebookEntryStatusDto::Draft);
    assert!(dto.parent_id.is_none());
}

#[test]
fn create_entry_with_parent() {
    let conn = setup();
    let parent = create_entry(
        &conn,
        "case-1",
        "investigator",
        &NotebookEntryTypeDto::Observation,
        "Parent",
        "Parent body",
        &[],
        &NotebookEntryStatusDto::Draft,
        None,
    )
    .unwrap();

    let child = create_entry(
        &conn,
        "case-1",
        "investigator",
        &NotebookEntryTypeDto::Hypothesis,
        "Child hypothesis",
        "Child body",
        &[],
        &NotebookEntryStatusDto::Draft,
        Some(&parent.id),
    )
    .unwrap();

    assert_eq!(child.parent_id.as_deref(), Some(parent.id.as_str()));
}

#[test]
fn update_entry_fields() {
    let conn = setup();
    let dto = create_entry(
        &conn,
        "case-1",
        "investigator",
        &NotebookEntryTypeDto::Observation,
        "Original title",
        "Original body",
        &["tag-a".to_string()],
        &NotebookEntryStatusDto::Draft,
        None,
    )
    .unwrap();

    let updated = update_entry(
        &conn,
        &dto.id,
        Some("Updated title"),
        Some("Updated body"),
        Some(&["tag-b".to_string()]),
        Some(&NotebookEntryStatusDto::Reviewed),
    )
    .unwrap();

    assert_eq!(updated.id, dto.id);
    assert_eq!(updated.title, "Updated title");
    assert_eq!(updated.body_markdown, "Updated body");
    assert_eq!(updated.tags, vec!["tag-b"]);
    assert_eq!(updated.status, NotebookEntryStatusDto::Reviewed);
}

#[test]
fn update_entry_partial() {
    let conn = setup();
    let dto = create_entry(
        &conn,
        "case-1",
        "investigator",
        &NotebookEntryTypeDto::Observation,
        "Title",
        "Body",
        &["original-tag".to_string()],
        &NotebookEntryStatusDto::Draft,
        None,
    )
    .unwrap();

    // Update only title
    let updated = update_entry(&conn, &dto.id, Some("New title"), None, None, None).unwrap();

    assert_eq!(updated.title, "New title");
    assert_eq!(updated.body_markdown, "Body"); // unchanged
    assert_eq!(updated.tags, vec!["original-tag"]); // unchanged
    assert_eq!(updated.status, NotebookEntryStatusDto::Draft); // unchanged
}

#[test]
fn list_entries_with_filters() {
    let conn = setup();
    create_entry(
        &conn,
        "case-1",
        "a",
        &NotebookEntryTypeDto::Finding,
        "Finding One",
        "body",
        &[],
        &NotebookEntryStatusDto::Draft,
        None,
    )
    .unwrap();
    create_entry(
        &conn,
        "case-1",
        "a",
        &NotebookEntryTypeDto::Observation,
        "Observation ABC",
        "body with searchable text",
        &[],
        &NotebookEntryStatusDto::Final,
        None,
    )
    .unwrap();
    create_entry(
        &conn,
        "case-1",
        "a",
        &NotebookEntryTypeDto::Conclusion,
        "Conclusion XYZ",
        "another body",
        &[],
        &NotebookEntryStatusDto::Draft,
        None,
    )
    .unwrap();

    // All entries
    let all = list_entries(&conn, "case-1", &NotebookEntryFilters::default()).unwrap();
    assert_eq!(all.len(), 3);

    // Filter by entry type
    let filters = NotebookEntryFilters {
        entry_type: Some(NotebookEntryType::Finding),
        ..Default::default()
    };
    let filtered = list_entries(&conn, "case-1", &filters).unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].entry_type, NotebookEntryTypeDto::Finding);

    // Filter by status
    let filters = NotebookEntryFilters {
        status: Some(EntryStatus::Final),
        ..Default::default()
    };
    let filtered = list_entries(&conn, "case-1", &filters).unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].status, NotebookEntryStatusDto::Final);

    // Search
    let filters = NotebookEntryFilters {
        search: Some("searchable".to_string()),
        ..Default::default()
    };
    let filtered = list_entries(&conn, "case-1", &filters).unwrap();
    assert_eq!(filtered.len(), 1);
    assert!(filtered[0].body_markdown.contains("searchable"));
}

#[test]
fn get_thread_returns_full_parent_chain_and_replies() {
    let conn = setup();
    let root = create_entry(
        &conn,
        "case-1",
        "a",
        &NotebookEntryTypeDto::Observation,
        "Root",
        "root body",
        &[],
        &NotebookEntryStatusDto::Draft,
        None,
    )
    .unwrap();
    let child1 = create_entry(
        &conn,
        "case-1",
        "a",
        &NotebookEntryTypeDto::Hypothesis,
        "Child 1",
        "child1 body",
        &[],
        &NotebookEntryStatusDto::Draft,
        Some(&root.id),
    )
    .unwrap();
    let child2 = create_entry(
        &conn,
        "case-1",
        "a",
        &NotebookEntryTypeDto::Finding,
        "Child 2",
        "child2 body",
        &[],
        &NotebookEntryStatusDto::Draft,
        Some(&child1.id),
    )
    .unwrap();

    // Starting from leaf (child2), should get root + child1 + child2
    let thread = get_thread(&conn, &child2.id).unwrap();
    assert_eq!(thread.len(), 3);
    assert_eq!(thread[0].id, root.id);
    assert_eq!(thread[1].id, child1.id);
    assert_eq!(thread[2].id, child2.id);

    // Starting from root should also work
    let thread = get_thread(&conn, &root.id).unwrap();
    assert_eq!(thread.len(), 3);
}

#[test]
fn add_citation_and_retrieve() {
    let conn = setup();
    let entry = create_entry(
        &conn,
        "case-1",
        "a",
        &NotebookEntryTypeDto::Finding,
        "Entry with citations",
        "body",
        &[],
        &NotebookEntryStatusDto::Draft,
        None,
    )
    .unwrap();

    let citation = add_citation(
        &conn,
        &entry.id,
        &GraphNodeTypeDto::File,
        "node-file-1",
        "cmd.exe hash mismatch",
        Some("SHA256: abcd1234"),
    )
    .unwrap();

    assert_eq!(citation.entry_id, entry.id);
    assert_eq!(citation.target_node_type, GraphNodeTypeDto::File);
    assert_eq!(citation.target_node_id, "node-file-1");
    assert_eq!(citation.display_label, "cmd.exe hash mismatch");
    assert_eq!(citation.snippet.as_deref(), Some("SHA256: abcd1234"));

    // Verify via repo
    let repo = NotebookRepo::new(&conn);
    let citations = repo.list_citations_for_entry(&entry.id).unwrap();
    assert_eq!(citations.len(), 1);
    assert_eq!(citations[0].id, citation.id);
}

#[test]
fn record_and_list_steps() {
    let conn = setup();
    let s1 = record_step(
        &conn,
        "case-1",
        "import",
        r#"{"source":"disk.img"}"#,
        "2026-06-14T12:00:00Z",
        5000,
        true,
        None,
        None,
    )
    .unwrap();
    let s2 = record_step(
        &conn,
        "case-1",
        "search",
        r#"{"query":"malware"}"#,
        "2026-06-14T12:01:00Z",
        150,
        true,
        None,
        Some("abc123hash"),
    )
    .unwrap();
    let s3 = record_step(
        &conn,
        "case-1",
        "artifact_extract",
        r#"{"family":"LNK"}"#,
        "2026-06-14T12:02:00Z",
        0,
        false,
        Some("E_PARSE_FAILED"),
        None,
    )
    .unwrap();

    // List all
    let all = list_steps(&conn, "case-1", &StepFilters::default()).unwrap();
    assert_eq!(all.len(), 3);

    assert_eq!(s1.step_kind, "import");
    assert_eq!(s1.duration_ms, 5000);
    assert!(s1.success);

    assert_eq!(s2.case_state_hash.as_deref(), Some("abc123hash"));

    assert!(!s3.success);
    assert_eq!(s3.error_code.as_deref(), Some("E_PARSE_FAILED"));

    // Filter by step_kind
    let filters = StepFilters {
        step_kind: Some("search".to_string()),
        ..Default::default()
    };
    let filtered = list_steps(&conn, "case-1", &filters).unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].step_kind, "search");

    // Filter by success
    let filters = StepFilters {
        success: Some(false),
        ..Default::default()
    };
    let filtered = list_steps(&conn, "case-1", &filters).unwrap();
    assert_eq!(filtered.len(), 1);
    assert!(!filtered[0].success);
}
