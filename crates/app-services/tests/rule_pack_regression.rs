mod entity_phase4_support;

use std::collections::BTreeMap;

use app_services::rule_pack::{
    execute_rule_pack, execute_rule_pack_incremental, parse_rule_pack, validate_rule_pack,
    V2_STANDARD_TOML,
};
use domain::{EdgeType, NodeType};
use entity_phase4_support::{case_db, insert_artifact, insert_graph_node, seed_file, CASE_ID};
use persistence_sqlite::repositories::graph_repo::GraphRepo;
use serde_json::Value;

const MINIMAL_PACK: &str = r#"
[manifest]
name = "minimal"
version = "1.0.0"
author = "test"
description = "minimal pack"
scope = ["test"]
min_product_version = "0.1.0"

[[rules]]
id = "lnk-path"
name = "LNK path"
description = "match path"
source_type = "artifact"
source_family = "LNK"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "target_path"
operator = "path_equals"
target_field = "path"

[rules.match_signals]
confidence = "direct"
"#;

#[test]
fn parser_preserves_builtin_contract() {
    let pack = parse_rule_pack(V2_STANDARD_TOML).expect("parse built-in pack");
    assert_eq!(pack.manifest.name, "v2-standard");
    assert_eq!(pack.rules.len(), 10);
    assert_eq!(pack.rules[0].id, "lnk-path-match");
}

#[test]
fn parser_reports_invalid_toml() {
    let errors = parse_rule_pack("invalid {{{").expect_err("invalid TOML must fail");
    assert!(!errors.is_empty());
}

#[test]
fn parser_accepts_operator_and_edge_enum_variants() {
    for operator in [
        "equals",
        "contains",
        "regex",
        "path_equals",
        "filename_equals",
        "temporal_proximity",
    ] {
        let source = MINIMAL_PACK.replace(
            "operator = \"path_equals\"",
            &format!("operator = \"{operator}\""),
        );
        assert!(parse_rule_pack(&source).is_ok(), "operator {operator}");
    }
    for edge in [
        "contains",
        "references",
        "correlates_with",
        "derives_from",
        "precedes",
        "cites",
        "annotates",
    ] {
        let source = MINIMAL_PACK.replace(
            "edge_type = \"correlates_with\"",
            &format!("edge_type = \"{edge}\""),
        );
        assert!(parse_rule_pack(&source).is_ok(), "edge {edge}");
    }
}

#[test]
fn parser_accepts_empty_rules_and_optional_caveats() {
    let source = r#"
[manifest]
name = "empty"
version = "1.0.0"
author = "test"
description = "empty"
scope = ["test"]
min_product_version = "0.1.0"
"#;
    let pack = parse_rule_pack(source).expect("parse empty pack");
    assert!(pack.rules.is_empty());
    let minimal = parse_rule_pack(MINIMAL_PACK).expect("parse minimal pack");
    assert!(minimal.rules[0].match_signals.caveats.is_none());
}

#[test]
fn validator_rejects_duplicate_unknown_and_temporal_rules() {
    let source = MINIMAL_PACK
        .replace("source_family = \"LNK\"", "source_family = \"Unknown\"")
        .replace(
            "operator = \"path_equals\"",
            "operator = \"temporal_proximity\"",
        );
    let mut pack = parse_rule_pack(&source).expect("parse invalid semantic pack");
    pack.rules.push(pack.rules[0].clone());
    let errors = validate_rule_pack(&pack);
    assert!(errors
        .iter()
        .any(|error| error.contains("duplicate rule id")));
    assert!(errors
        .iter()
        .any(|error| error.contains("unknown source_family")));
    assert!(errors
        .iter()
        .any(|error| error.contains("temporal_proximity")));
}

#[test]
fn validator_rejects_empty_manifest_and_unknown_confidence() {
    let source = MINIMAL_PACK
        .replace("name = \"minimal\"", "name = \"\"")
        .replace("scope = [\"test\"]", "scope = []")
        .replace("confidence = \"direct\"", "confidence = \"certain\"");
    let pack = parse_rule_pack(&source).expect("parse semantic errors");
    let errors = validate_rule_pack(&pack);
    assert!(errors
        .iter()
        .any(|error| error.contains("manifest.name must not be empty")));
    assert!(errors
        .iter()
        .any(|error| error.contains("manifest.scope must contain")));
    assert!(errors
        .iter()
        .any(|error| error.contains("unknown confidence level")));
}

#[test]
fn path_rule_executes_and_projects_provenance() {
    let conn = case_db();
    insert_graph_node(&conn, "file-1", NodeType::File, "cmd.exe", &[]);
    seed_file(&conn, "file-1", "C:/Windows/System32/cmd.exe", "cmd.exe");
    let mut attrs = BTreeMap::new();
    attrs.insert(
        "target_path".to_string(),
        Value::String("c:\\windows\\system32\\cmd.exe".to_string()),
    );
    insert_artifact(&conn, "artifact-lnk", "LNK", "LNK", "", attrs);

    let pack = parse_rule_pack(MINIMAL_PACK).expect("parse pack");
    assert!(validate_rule_pack(&pack).is_empty());
    assert_eq!(
        execute_rule_pack(&conn, CASE_ID, &pack).expect("execute pack"),
        1
    );
    let (_, edges) = GraphRepo::new(&conn)
        .traverse(
            &["artifact-lnk".to_string()],
            &[EdgeType::CorrelatesWith],
            1,
            10,
        )
        .expect("read graph edges");
    assert_eq!(edges.len(), 1);
    let provenance: Value =
        serde_json::from_str(edges[0].provenance.as_deref().expect("provenance"))
            .expect("parse provenance");
    assert_eq!(provenance["pack_id"], "minimal");
    assert_eq!(provenance["rule_id"], "lnk-path");
}

#[test]
fn filename_rule_matches_prefetch_executable() {
    let conn = case_db();
    insert_graph_node(&conn, "file-1", NodeType::File, "cmd.exe", &[]);
    seed_file(&conn, "file-1", "C:/Windows/System32/cmd.exe", "cmd.exe");
    let mut attrs = BTreeMap::new();
    attrs.insert(
        "executable".to_string(),
        Value::String("CMD.EXE".to_string()),
    );
    insert_artifact(
        &conn,
        "artifact-prefetch",
        "Prefetch",
        "Prefetch",
        "",
        attrs,
    );
    let source = MINIMAL_PACK
        .replace("source_family = \"LNK\"", "source_family = \"Prefetch\"")
        .replace("field = \"target_path\"", "field = \"executable\"")
        .replace(
            "operator = \"path_equals\"",
            "operator = \"filename_equals\"",
        )
        .replace("target_field = \"path\"", "target_field = \"name\"");
    let pack = parse_rule_pack(&source).expect("parse prefetch pack");
    assert_eq!(
        execute_rule_pack(&conn, CASE_ID, &pack).expect("execute prefetch pack"),
        1
    );
}

#[test]
fn incremental_execution_skips_completed_rule() {
    let conn = case_db();
    insert_graph_node(&conn, "file-1", NodeType::File, "cmd.exe", &[]);
    seed_file(&conn, "file-1", "C:/Windows/System32/cmd.exe", "cmd.exe");
    let mut attrs = BTreeMap::new();
    attrs.insert(
        "target_path".to_string(),
        Value::String("C:/Windows/System32/cmd.exe".to_string()),
    );
    insert_artifact(&conn, "artifact-lnk", "LNK", "LNK", "", attrs);
    let pack = parse_rule_pack(MINIMAL_PACK).expect("parse pack");

    assert_eq!(
        execute_rule_pack(&conn, CASE_ID, &pack).expect("initial execution"),
        1
    );
    assert_eq!(
        execute_rule_pack_incremental(&conn, CASE_ID, &pack).expect("incremental execution"),
        0
    );
}

#[test]
fn unsupported_source_types_return_zero() {
    let source = MINIMAL_PACK
        .replace(
            "source_type = \"artifact\"",
            "source_type = \"timeline_event\"",
        )
        .replace("source_family = \"LNK\"", "source_family = \"EvtxEvent\"");
    let conn = case_db();
    let pack = parse_rule_pack(&source).expect("parse timeline rule");
    assert_eq!(
        execute_rule_pack(&conn, CASE_ID, &pack).expect("execute unsupported rule"),
        0
    );
}

#[test]
fn missing_source_artifacts_and_builtin_pack_are_safe() {
    let conn = case_db();
    let minimal = parse_rule_pack(MINIMAL_PACK).expect("parse minimal pack");
    assert_eq!(
        execute_rule_pack(&conn, CASE_ID, &minimal).expect("execute empty pack inputs"),
        0
    );

    insert_graph_node(&conn, "file-1", NodeType::File, "cmd.exe", &[]);
    seed_file(&conn, "file-1", "C:/Windows/System32/cmd.exe", "cmd.exe");
    let mut attrs = BTreeMap::new();
    attrs.insert(
        "target_path".to_string(),
        Value::String("C:/Windows/System32/cmd.exe".to_string()),
    );
    insert_artifact(&conn, "artifact-lnk", "LNK", "LNK", "", attrs);
    let builtin = parse_rule_pack(V2_STANDARD_TOML).expect("parse built-in pack");
    assert!(execute_rule_pack(&conn, CASE_ID, &builtin).expect("execute built-in pack") > 0);
}
