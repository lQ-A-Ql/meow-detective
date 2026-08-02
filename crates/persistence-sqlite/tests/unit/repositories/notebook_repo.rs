use super::*;
use crate::{connection::open_in_memory, runner};

fn setup() -> (&'static Connection, NotebookRepo<'static>) {
    let conn = Box::new(open_in_memory().unwrap());
    let conn_ref: &'static Connection = Box::leak(conn);
    runner::run_all(conn_ref).unwrap();
    // Insert a dummy case for foreign key
    conn_ref
        .execute(
            "INSERT INTO cases (id, name, created_at, updated_at) VALUES ('case-1', 'Test', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    let repo = NotebookRepo::new(conn_ref);
    (conn_ref, repo)
}

fn make_entry(
    id: &str,
    case_id: &str,
    parent_id: Option<&str>,
    entry_type: NotebookEntryType,
    title: &str,
    status: EntryStatus,
) -> NotebookEntry {
    NotebookEntry {
        id: id.to_string(),
        case_id: case_id.to_string(),
        parent_id: parent_id.map(|s| s.to_string()),
        author: "tester".to_string(),
        entry_type,
        title: title.to_string(),
        body_markdown: format!("Body for {id}"),
        tags: vec!["test".to_string()],
        status,
        created_at: "2026-06-14T00:00:00Z".to_string(),
        updated_at: "2026-06-14T00:00:00Z".to_string(),
    }
}

fn make_citation(
    id: &str,
    entry_id: &str,
    target_node_type: NodeType,
    target_node_id: &str,
) -> EvidenceCitation {
    EvidenceCitation {
        id: id.to_string(),
        entry_id: entry_id.to_string(),
        target_node_type,
        target_node_id: target_node_id.to_string(),
        display_label: format!("cite-{id}"),
        snippet: Some("relevant snippet".to_string()),
        cited_at: "2026-06-14T00:00:00Z".to_string(),
    }
}

fn make_step(id: &str, case_id: &str, step_kind: &str) -> InvestigationStep {
    InvestigationStep {
        id: id.to_string(),
        case_id: case_id.to_string(),
        step_kind: step_kind.to_string(),
        params_json: r#"{"key":"value"}"#.to_string(),
        timestamp: "2026-06-14T00:00:00Z".to_string(),
        duration_ms: Some(1500),
        case_state_hash: None,
        success: Some(true),
        error_code: None,
    }
}

#[test]
fn create_and_get_entry() {
    let (_conn, repo) = setup();
    let entry = make_entry(
        "e1",
        "case-1",
        None,
        NotebookEntryType::Finding,
        "A Finding",
        EntryStatus::Draft,
    );
    repo.create_entry(&entry).unwrap();

    let fetched = repo.get_entry("e1").unwrap().expect("entry should exist");
    assert_eq!(fetched.id, "e1");
    assert_eq!(fetched.title, "A Finding");
    assert_eq!(fetched.entry_type, NotebookEntryType::Finding);
    assert_eq!(fetched.status, EntryStatus::Draft);
    assert_eq!(fetched.tags, vec!["test"]);
}

#[test]
fn update_entry_fields() {
    let (_conn, repo) = setup();
    let entry = make_entry(
        "e1",
        "case-1",
        None,
        NotebookEntryType::Observation,
        "Original",
        EntryStatus::Draft,
    );
    repo.create_entry(&entry).unwrap();

    repo.update_entry(
        "e1",
        Some("Updated Title"),
        Some("Updated body"),
        None,
        Some(&EntryStatus::Reviewed),
        "2026-06-14T01:00:00Z",
    )
    .unwrap();

    let fetched = repo.get_entry("e1").unwrap().unwrap();
    assert_eq!(fetched.title, "Updated Title");
    assert_eq!(fetched.body_markdown, "Updated body");
    assert_eq!(fetched.status, EntryStatus::Reviewed);
    assert_eq!(fetched.updated_at, "2026-06-14T01:00:00Z");
    // tags not updated
    assert_eq!(fetched.tags, vec!["test"]);
}

#[test]
fn list_entries_with_filters() {
    let (_conn, repo) = setup();
    let e1 = make_entry(
        "e1",
        "case-1",
        None,
        NotebookEntryType::Finding,
        "Finding One",
        EntryStatus::Draft,
    );
    let e2 = make_entry(
        "e2",
        "case-1",
        None,
        NotebookEntryType::Observation,
        "Observation Two",
        EntryStatus::Final,
    );
    let e3 = make_entry(
        "e3",
        "case-1",
        None,
        NotebookEntryType::Observation,
        "Observation Three",
        EntryStatus::Draft,
    );
    repo.create_entry(&e1).unwrap();
    repo.create_entry(&e2).unwrap();
    repo.create_entry(&e3).unwrap();

    // Filter by entry type
    let filters = NotebookEntryFilters {
        entry_type: Some(NotebookEntryType::Observation),
        ..Default::default()
    };
    let results = repo.list_entries("case-1", &filters).unwrap();
    assert_eq!(results.len(), 2);

    // Filter by status
    let filters = NotebookEntryFilters {
        status: Some(EntryStatus::Final),
        ..Default::default()
    };
    let results = repo.list_entries("case-1", &filters).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "e2");

    // Search
    let filters = NotebookEntryFilters {
        search: Some("Three".to_string()),
        ..Default::default()
    };
    let results = repo.list_entries("case-1", &filters).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "e3");
}

#[test]
fn get_thread_recursive_cte() {
    let (_conn, repo) = setup();
    let root = make_entry(
        "root",
        "case-1",
        None,
        NotebookEntryType::Observation,
        "Root",
        EntryStatus::Draft,
    );
    let child1 = make_entry(
        "child1",
        "case-1",
        Some("root"),
        NotebookEntryType::Hypothesis,
        "Child 1",
        EntryStatus::Draft,
    );
    let child2 = make_entry(
        "child2",
        "case-1",
        Some("child1"),
        NotebookEntryType::Finding,
        "Child 2",
        EntryStatus::Draft,
    );
    repo.create_entry(&root).unwrap();
    repo.create_entry(&child1).unwrap();
    repo.create_entry(&child2).unwrap();

    let thread = repo.get_thread("root").unwrap();
    assert_eq!(thread.len(), 3);
    assert_eq!(thread[0].id, "root");
    assert_eq!(thread[1].id, "child1");
    assert_eq!(thread[2].id, "child2");
}

#[test]
fn soft_delete_entry() {
    let (_conn, repo) = setup();
    let entry = make_entry(
        "e1",
        "case-1",
        None,
        NotebookEntryType::Observation,
        "To Delete",
        EntryStatus::Draft,
    );
    repo.create_entry(&entry).unwrap();

    repo.delete_entry("e1", "2026-06-15T00:00:00Z").unwrap();
    let fetched = repo.get_entry("e1").unwrap().unwrap();
    assert_eq!(fetched.status, EntryStatus::Draft); // parse maps 'deleted' → Draft
    assert_eq!(fetched.body_markdown, "");
    assert_eq!(fetched.updated_at, "2026-06-15T00:00:00Z");
}

#[test]
fn add_and_list_citations() {
    let (_conn, repo) = setup();
    let entry = make_entry(
        "e1",
        "case-1",
        None,
        NotebookEntryType::Finding,
        "With Citations",
        EntryStatus::Draft,
    );
    repo.create_entry(&entry).unwrap();

    let c1 = make_citation("c1", "e1", NodeType::File, "node-1");
    let c2 = make_citation("c2", "e1", NodeType::Artifact, "node-2");
    repo.add_citation(&c1).unwrap();
    repo.add_citation(&c2).unwrap();

    let citations = repo.list_citations_for_entry("e1").unwrap();
    assert_eq!(citations.len(), 2);

    repo.remove_citation("c1").unwrap();
    let citations = repo.list_citations_for_entry("e1").unwrap();
    assert_eq!(citations.len(), 1);
    assert_eq!(citations[0].id, "c2");
}

#[test]
fn record_and_list_steps() {
    let (_conn, repo) = setup();
    let s1 = make_step("s1", "case-1", "import");
    let s2 = make_step("s2", "case-1", "search");
    repo.record_step(&s1).unwrap();
    repo.record_step(&s2).unwrap();

    let all = repo.list_steps("case-1", &StepFilters::default()).unwrap();
    assert_eq!(all.len(), 2);

    // Filter by step_kind
    let filters = StepFilters {
        step_kind: Some("import".to_string()),
        ..Default::default()
    };
    let filtered = repo.list_steps("case-1", &filters).unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "s1");
    assert_eq!(filtered[0].success, Some(true));
}

#[test]
fn batch_citations_maps_correctly() {
    let (_conn, repo) = setup();
    let e1 = make_entry(
        "e1",
        "case-1",
        None,
        NotebookEntryType::Finding,
        "E1",
        EntryStatus::Draft,
    );
    let e2 = make_entry(
        "e2",
        "case-1",
        None,
        NotebookEntryType::Finding,
        "E2",
        EntryStatus::Draft,
    );
    repo.create_entry(&e1).unwrap();
    repo.create_entry(&e2).unwrap();

    let c1 = make_citation("c1", "e1", NodeType::File, "node-1");
    let c2 = make_citation("c2", "e1", NodeType::Artifact, "node-2");
    let c3 = make_citation("c3", "e2", NodeType::Entity, "node-3");
    repo.add_citation(&c1).unwrap();
    repo.add_citation(&c2).unwrap();
    repo.add_citation(&c3).unwrap();

    let map = repo
        .batch_citations_for_entries(&["e1".to_string(), "e2".to_string()])
        .unwrap();
    assert_eq!(map.len(), 2);
    assert_eq!(map.get("e1").map(|v| v.len()), Some(2));
    assert_eq!(map.get("e2").map(|v| v.len()), Some(1));
}

#[test]
fn get_nonexistent_entry_returns_none() {
    let (_conn, repo) = setup();
    let result = repo.get_entry("no-such").unwrap();
    assert!(result.is_none());
}

#[test]
fn delete_case_notebook_removes_all() {
    let (_conn, repo) = setup();
    let e1 = make_entry(
        "e1",
        "case-1",
        None,
        NotebookEntryType::Finding,
        "E1",
        EntryStatus::Draft,
    );
    repo.create_entry(&e1).unwrap();
    let c1 = make_citation("c1", "e1", NodeType::File, "node-1");
    repo.add_citation(&c1).unwrap();

    repo.delete_case_notebook("case-1").unwrap();

    let entries = repo
        .list_entries("case-1", &NotebookEntryFilters::default())
        .unwrap();
    assert_eq!(entries.len(), 0);
}

#[test]
fn case_counts_exclude_deleted_entries_and_other_cases() {
    let (conn, repo) = setup();
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at) VALUES ('case-2', 'Other', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();

    for entry in [
        make_entry(
            "active-1",
            "case-1",
            None,
            NotebookEntryType::Finding,
            "Active",
            EntryStatus::Draft,
        ),
        make_entry(
            "deleted-1",
            "case-1",
            None,
            NotebookEntryType::Observation,
            "Deleted",
            EntryStatus::Draft,
        ),
        make_entry(
            "other-1",
            "case-2",
            None,
            NotebookEntryType::Finding,
            "Other case",
            EntryStatus::Draft,
        ),
    ] {
        repo.create_entry(&entry).unwrap();
    }

    for citation in [
        make_citation("active-citation", "active-1", NodeType::File, "node-1"),
        make_citation("deleted-citation", "deleted-1", NodeType::File, "node-2"),
        make_citation("other-citation", "other-1", NodeType::File, "node-3"),
    ] {
        repo.add_citation(&citation).unwrap();
    }
    repo.delete_entry("deleted-1", "2026-06-15T00:00:00Z")
        .unwrap();

    assert_eq!(repo.count_active_entries_for_case("case-1").unwrap(), 1);
    assert_eq!(repo.count_citations_for_case("case-1").unwrap(), 1);
    assert_eq!(repo.count_active_entries_for_case("case-2").unwrap(), 1);
    assert_eq!(repo.count_citations_for_case("case-2").unwrap(), 1);
    assert_eq!(repo.count_active_entries_for_case("missing").unwrap(), 0);
    assert_eq!(repo.count_citations_for_case("missing").unwrap(), 0);
}
