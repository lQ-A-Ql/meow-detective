use gql::parser::{parse, ComparisonOp, LogicalConnector, MatchDirection, ReturnItem, Value};

#[test]
fn parse_simple_match_with_types() {
    let q = parse("MATCH (n:File)-[e:References]->(m:Artifact) RETURN n").unwrap();
    assert_eq!(q.match_clause.source_var, "n");
    assert_eq!(q.match_clause.source_type, Some("File".to_string()));
    assert_eq!(q.match_clause.edge_var, "e");
    assert_eq!(q.match_clause.edge_type, Some("References".to_string()));
    assert_eq!(q.match_clause.target_var, "m");
    assert_eq!(q.match_clause.target_type, Some("Artifact".to_string()));
    assert_eq!(q.match_clause.direction, MatchDirection::LeftToRight);
    assert_eq!(q.return_clause.items.len(), 1);
    assert!(q.where_clause.is_none());
}

#[test]
fn parse_match_no_type_annotations() {
    let q = parse("MATCH (a)-[r]->(b) RETURN a, b").unwrap();
    assert_eq!(q.match_clause.source_var, "a");
    assert_eq!(q.match_clause.source_type, None);
    assert_eq!(q.match_clause.edge_var, "r");
    assert_eq!(q.match_clause.edge_type, None);
    assert_eq!(q.match_clause.target_var, "b");
    assert_eq!(q.match_clause.target_type, None);
}

#[test]
fn parse_match_reverse_direction() {
    let q = parse("MATCH (a:File)<-[r:References]-(b:File) RETURN a").unwrap();
    assert_eq!(q.match_clause.direction, MatchDirection::RightToLeft);
    assert_eq!(q.match_clause.source_type, Some("File".to_string()));
    assert_eq!(q.match_clause.target_type, Some("File".to_string()));
}

#[test]
fn parse_reverse_direction_bare_arrow() {
    let q = parse("MATCH (a)<--(b) RETURN a, b").unwrap();
    assert_eq!(q.match_clause.direction, MatchDirection::RightToLeft);
    assert_eq!(q.match_clause.source_var, "a");
}

#[test]
fn parse_where_single_predicate_eq_string() {
    let q = parse("MATCH (n:File)-[e]->(m) WHERE n.label = 'cmd.exe' RETURN n").unwrap();
    let w = q.where_clause.as_ref().unwrap();
    assert_eq!(w.predicates.len(), 1);
    assert_eq!(w.predicates[0].variable, "n");
    assert_eq!(w.predicates[0].property, "label");
    assert_eq!(w.predicates[0].operator, ComparisonOp::Eq);
    assert_eq!(w.predicates[0].value, Value::String("cmd.exe".to_string()));
}

#[test]
fn parse_where_predicate_gt_number() {
    let q = parse("MATCH (n)-[e]->(m) WHERE e.confidence > 0.7 RETURN n, e, m").unwrap();
    let w = q.where_clause.as_ref().unwrap();
    assert_eq!(w.predicates[0].operator, ComparisonOp::Gt);
    assert_eq!(w.predicates[0].value, Value::Number(0.7));
}

#[test]
fn parse_where_predicate_gte_number() {
    let q = parse("MATCH (n)-[e]->(m) WHERE e.confidence >= 0.5 RETURN n").unwrap();
    let w = q.where_clause.as_ref().unwrap();
    assert_eq!(w.predicates[0].operator, ComparisonOp::Gte);
}

#[test]
fn parse_where_predicate_lte_number() {
    let q = parse("MATCH (n)-[e]->(m) WHERE e.confidence <= 0.9 RETURN n").unwrap();
    let w = q.where_clause.as_ref().unwrap();
    assert_eq!(w.predicates[0].operator, ComparisonOp::Lte);
}

#[test]
fn parse_where_multiple_predicates_and() {
    let q = parse("MATCH (n)-[e]->(m) WHERE n.label = 'cmd.exe' AND e.confidence > 0.7 RETURN n")
        .unwrap();
    let w = q.where_clause.as_ref().unwrap();
    assert_eq!(w.predicates.len(), 2);
    assert_eq!(w.connector, LogicalConnector::And);
}

#[test]
fn parse_where_multiple_predicates_or() {
    let q = parse("MATCH (n)-[e]->(m) WHERE n.label = 'a' OR n.label = 'b' RETURN n").unwrap();
    let w = q.where_clause.as_ref().unwrap();
    assert_eq!(w.connector, LogicalConnector::Or);
}

#[test]
fn parse_where_like_operator() {
    let q = parse("MATCH (n:File)-[e]->(m) WHERE n.label LIKE '%cmd%' RETURN n").unwrap();
    let w = q.where_clause.as_ref().unwrap();
    assert_eq!(w.predicates[0].operator, ComparisonOp::Like);
}

#[test]
fn parse_where_contains_operator() {
    let q = parse("MATCH (n:File)-[e]->(m) WHERE n.tags CONTAINS 'executable' RETURN n").unwrap();
    let w = q.where_clause.as_ref().unwrap();
    assert_eq!(w.predicates[0].operator, ComparisonOp::Contains);
}

#[test]
fn parse_where_not_eq() {
    let q = parse("MATCH (n)-[e]->(m) WHERE n.label != 'test' RETURN n").unwrap();
    let w = q.where_clause.as_ref().unwrap();
    assert_eq!(w.predicates[0].operator, ComparisonOp::Neq);
}

#[test]
fn parse_return_count_star() {
    let q = parse("MATCH (n)-[e]->(m) RETURN count(*)").unwrap();
    assert_eq!(q.return_clause.items.len(), 1);
    assert_eq!(q.return_clause.items[0], ReturnItem::CountStar);
}

#[test]
fn parse_return_count_var() {
    let q = parse("MATCH (n)-[e]->(m) RETURN count(n)").unwrap();
    assert_eq!(q.return_clause.items[0], ReturnItem::Count("n".to_string()));
}

#[test]
fn parse_return_multiple_items() {
    let q = parse("MATCH (n)-[e]->(m) RETURN n, e, m, count(*)").unwrap();
    assert_eq!(q.return_clause.items.len(), 4);
}

#[test]
fn parse_limit() {
    let q = parse("MATCH (n)-[e]->(m) RETURN n LIMIT 100").unwrap();
    assert_eq!(q.limit, Some(100));
}

#[test]
fn parse_no_limit() {
    let q = parse("MATCH (n)-[e]->(m) RETURN n").unwrap();
    assert_eq!(q.limit, None);
}

#[test]
fn parse_case_insensitive_keywords() {
    let q =
        parse("match (n:File)-[e:References]->(m:Artifact) where n.label = 'x' return n limit 10")
            .unwrap();
    assert_eq!(q.match_clause.source_type, Some("File".to_string()));
    assert!(q.where_clause.is_some());
    assert_eq!(q.limit, Some(10));
}

#[test]
fn parse_bare_edge_no_brackets() {
    let q = parse("MATCH (a)-->(b) RETURN a, b").unwrap();
    assert_eq!(q.match_clause.source_var, "a");
    assert_eq!(q.match_clause.target_var, "b");
    assert_eq!(q.match_clause.edge_var, "_");
    assert_eq!(q.match_clause.edge_type, None);
}

#[test]
fn parse_bool_values() {
    let q = parse("MATCH (n)-[e]->(m) WHERE e.confidence = true RETURN n").unwrap();
    let w = q.where_clause.as_ref().unwrap();
    assert_eq!(w.predicates[0].value, Value::Bool(true));
}

#[test]
fn parse_null_value() {
    let q = parse("MATCH (n)-[e]->(m) WHERE e.provenance = null RETURN n").unwrap();
    let w = q.where_clause.as_ref().unwrap();
    assert_eq!(w.predicates[0].value, Value::Null);
}

#[test]
fn parse_with_aggregate_min() {
    let q = parse("MATCH (n)-[e]->(m) RETURN min(e.confidence)").unwrap();
    assert_eq!(
        q.return_clause.items[0],
        ReturnItem::Aggregate {
            func: "min".to_string(),
            variable: "e.confidence".to_string()
        }
    );
}

#[test]
fn parse_with_aggregate_max() {
    let q = parse("MATCH (n)-[e]->(m) RETURN max(e.confidence)").unwrap();
    assert_eq!(
        q.return_clause.items[0],
        ReturnItem::Aggregate {
            func: "max".to_string(),
            variable: "e.confidence".to_string()
        }
    );
}

#[test]
fn parse_error_on_invalid() {
    let result = parse("INVALID QUERY HERE");
    assert!(result.is_err());
}

#[test]
fn parse_error_unterminated_string() {
    let result = parse("MATCH (n)-[e]->(m) WHERE n.label = 'unterminated RETURN n");
    assert!(result.is_err());
}

#[test]
fn display_roundtrip_simple() {
    let input = "MATCH (n:File)-[e:References]->(m:Artifact) WHERE n.label = 'test' AND e.confidence > 0.5 RETURN n, e, m LIMIT 50";
    let q = parse(input).unwrap();
    let output = q.to_string();
    assert!(
        output.contains("MATCH"),
        "Display output '{}' should contain MATCH",
        output
    );
    assert!(
        output.contains("WHERE"),
        "Display output '{}' should contain WHERE",
        output
    );
    assert!(
        output.contains("RETURN"),
        "Display output '{}' should contain RETURN",
        output
    );
    assert!(
        output.contains("LIMIT"),
        "Display output '{}' should contain LIMIT",
        output
    );
}

#[test]
fn display_match_no_where_no_limit() {
    let q = parse("MATCH (n)-[e]->(m) RETURN n, e").unwrap();
    let out = q.to_string();
    assert!(out.starts_with("MATCH"));
    assert!(out.ends_with("RETURN n, e"));
}
