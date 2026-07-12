use super::*;

#[test]
fn plan_for_typed_match() {
    let query =
        crate::parser::parse("MATCH (n:File)-[e:References]->(m:Artifact) RETURN n, e, m").unwrap();
    let plan = estimate_plan(&query);
    assert!(plan.steps.len() >= 4);
    assert!(plan.steps[0].uses_index);
    assert!(plan.total_cost.estimated_ops > 0);
    assert!(!plan.summary.is_empty());
}

#[test]
fn plan_for_untyped_match_has_higher_cost() {
    let typed = estimate_plan(
        &crate::parser::parse("MATCH (n:File)-[e:References]->(m) RETURN n").unwrap(),
    );
    let untyped = estimate_plan(&crate::parser::parse("MATCH (n)-[e]->(m) RETURN n").unwrap());
    assert!(untyped.total_cost.nodes_visited > typed.total_cost.nodes_visited);
}

#[test]
fn plan_with_where_adds_filter_step() {
    let without = estimate_plan(&crate::parser::parse("MATCH (n:File)-[e]->(m) RETURN n").unwrap());
    let with = estimate_plan(
        &crate::parser::parse("MATCH (n:File)-[e]->(m) WHERE e.confidence > 0.7 RETURN n").unwrap(),
    );
    assert!(with.steps.len() > without.steps.len());
}

#[test]
fn plan_with_count_aggregate_has_project_step() {
    let plan = estimate_plan(&crate::parser::parse("MATCH (n)-[e]->(m) RETURN count(*)").unwrap());
    let step = plan
        .steps
        .iter()
        .find(|step| step.operation == "project")
        .expect("project step");
    assert!(step.description.contains("count(*)"));
}

#[test]
fn plan_with_limit() {
    let plan =
        estimate_plan(&crate::parser::parse("MATCH (n)-[e]->(m) RETURN n LIMIT 10").unwrap());
    let step = plan
        .steps
        .iter()
        .find(|step| step.operation == "limit")
        .expect("limit step");
    assert!(step.description.contains("10"));
}

#[test]
fn plan_total_cost_aggregates_all_steps() {
    let query = crate::parser::parse(
        "MATCH (n:File)-[e:References]->(m:Artifact) WHERE e.confidence > 0.5 RETURN n, m LIMIT 25",
    )
    .unwrap();
    let plan = estimate_plan(&query);
    let sum_ops: u64 = plan
        .steps
        .iter()
        .map(|step| step.estimated_cost.estimated_ops)
        .sum();
    assert_eq!(plan.total_cost.estimated_ops, sum_ops);
    assert!(plan.total_cost.nodes_visited > 0);
}

#[test]
fn plan_reverse_direction_uses_target_index() {
    let query =
        crate::parser::parse("MATCH (a:Artifact)<-[e:References]-(f:File) RETURN a, e, f").unwrap();
    let plan = estimate_plan(&query);
    let step = plan
        .steps
        .iter()
        .find(|step| step.operation == "traverse")
        .expect("traverse step");
    assert_eq!(step.index_name.as_deref(), Some("idx_graph_edges_target"));
}

#[test]
fn plan_outgoing_direction_uses_source_index() {
    let query =
        crate::parser::parse("MATCH (n:File)-[e:References]->(m:Artifact) RETURN n, e, m").unwrap();
    let plan = estimate_plan(&query);
    let step = plan
        .steps
        .iter()
        .find(|step| step.operation == "traverse")
        .expect("traverse step");
    assert_eq!(step.index_name.as_deref(), Some("idx_graph_edges_source"));
}
