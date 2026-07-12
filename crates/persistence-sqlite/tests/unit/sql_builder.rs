use super::*;

#[test]
fn placeholders_generates_sequential_list() {
    assert_eq!(placeholders(1, 3), "?1, ?2, ?3");
    assert_eq!(placeholders(4, 2), "?4, ?5");
    assert_eq!(placeholders(1, 0), "");
}

#[test]
fn clause_builder_tracks_param_indices() {
    let mut builder = ClauseBuilder::new();
    builder.push_eq("case_id", "case-1".to_string());
    builder.push_cmp("ts", ">=", "2025-01-01".to_string());
    assert_eq!(builder.where_clause(), "WHERE case_id = ?1 AND ts >= ?2");
    assert_eq!(builder.param_refs().len(), 2);
}

#[test]
fn empty_builder_produces_empty_where_clause() {
    let builder = ClauseBuilder::new();
    assert!(builder.is_empty());
    assert_eq!(builder.where_clause(), "");
}

#[test]
fn set_clause_joins_with_commas() {
    let mut builder = ClauseBuilder::new();
    builder.push_eq("title", "New title".to_string());
    builder.push_eq("updated_at", "2025-01-01".to_string());
    assert_eq!(builder.set_clause(), "title = ?1, updated_at = ?2");
}

#[test]
fn push_raw_appends_clause_and_values_after_existing_params() {
    let mut builder = ClauseBuilder::new();
    builder.push_eq("case_id", "case-1".to_string());
    let idx = builder.next_param();
    builder.push_raw(
        format!("(title LIKE ?{idx} OR body LIKE ?{})", idx + 1),
        vec!["%a%".to_string(), "%a%".to_string()],
    );
    assert_eq!(
        builder.where_clause(),
        "WHERE case_id = ?1 AND (title LIKE ?2 OR body LIKE ?3)"
    );
    assert_eq!(builder.param_refs().len(), 3);
}
