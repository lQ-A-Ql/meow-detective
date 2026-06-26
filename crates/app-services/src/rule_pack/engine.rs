use chrono::Utc;
use domain::{GraphEdge, NodeType};
use persistence_sqlite::repositories::{artifact_repo::ArtifactRepo, graph_repo::GraphRepo};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeMap;

use super::error::RulePackError;
use super::parser::{Operator, RulePack};

// ── Public API ──

/// Execute all rules in a rule pack against the case database.
///
/// For each rule, queries the source nodes (currently only Artifact sources are
/// supported), applies field predicates to find matching target nodes (File
/// entries), and creates `CorrelatesWith` graph edges with provenance metadata.
///
/// Returns the total number of graph edges created across all rules.
pub fn execute_rule_pack(
    conn: &Connection,
    case_id: &str,
    pack: &RulePack,
) -> Result<u64, RulePackError> {
    let mut total_edges: u64 = 0;

    for rule in &pack.rules {
        let edges = execute_rule(conn, case_id, pack, rule)?;
        total_edges += edges;
    }

    Ok(total_edges)
}

/// Execute only rules that have not yet been executed for this pack/case.
///
/// Queries existing `CorrelatesWith` edges with provenance matching the pack
/// and skips any rule whose id already appears in the provenance. This avoids
/// duplicate edge creation across repeated pack executions.
///
/// Returns the total number of new graph edges created.
pub fn execute_rule_pack_incremental(
    conn: &Connection,
    case_id: &str,
    pack: &RulePack,
) -> Result<u64, RulePackError> {
    let executed_rule_ids = get_executed_rule_ids(conn, case_id, &pack.manifest.name)?;
    let mut total_edges: u64 = 0;

    for rule in &pack.rules {
        if executed_rule_ids.contains(&rule.id) {
            continue;
        }
        let edges = execute_rule(conn, case_id, pack, rule)?;
        total_edges += edges;
    }

    Ok(total_edges)
}

// ── Internal execution ──

fn execute_rule(
    conn: &Connection,
    case_id: &str,
    pack: &RulePack,
    rule: &super::parser::RuleDefinition,
) -> Result<u64, RulePackError> {
    match rule.source_type {
        super::parser::NodeType::Artifact => execute_artifact_rule(conn, case_id, pack, rule),
        super::parser::NodeType::TimelineEvent => {
            // TimelineEvent rules (e.g., temporal proximity) are reserved for
            // a future rule pack version. Return 0 edges without error.
            Ok(0)
        }
        _ => {
            // Other source types (File, Entity, Lead, NotebookEntry) not yet
            // implemented in this engine version.
            Ok(0)
        }
    }
}

fn execute_artifact_rule(
    conn: &Connection,
    case_id: &str,
    pack: &RulePack,
    rule: &super::parser::RuleDefinition,
) -> Result<u64, RulePackError> {
    // 1. Load source artifacts of the matching family
    let artifacts = load_artifacts_by_family(conn, &rule.source_family)?;
    if artifacts.is_empty() {
        return Ok(0);
    }

    // 2. Load target nodes
    let domain_target_type: domain::NodeType = rule.target_type.clone().into();
    let target_nodes = load_nodes_by_type(conn, case_id, &domain_target_type)?;
    if target_nodes.is_empty() {
        return Ok(0);
    }

    // 3. For file targets, also load file entries for path/name matching
    let files = if domain_target_type == domain::NodeType::File {
        load_file_entries(conn)?
    } else {
        Vec::new()
    };

    // 4. Ensure artifact graph nodes exist (graph_edges FK references graph_nodes)
    let graph_repo = GraphRepo::new(conn);
    let now = Utc::now().to_rfc3339();
    let mut artifact_nodes: Vec<domain::GraphNode> = Vec::new();
    for artifact in &artifacts {
        artifact_nodes.push(domain::GraphNode {
            id: artifact.id.clone(),
            case_id: case_id.to_string(),
            node_type: domain::NodeType::Artifact,
            label: format!("{} artifact", rule.source_family),
            summary: format!("{} rule pack artifact", rule.source_family),
            tags: vec![],
            created_at: now.clone(),
        });
    }
    // Use INSERT OR REPLACE to avoid errors on duplicate artifact nodes
    let _ = graph_repo.insert_nodes_batch(&artifact_nodes);

    // 5. Apply conditions to find matches and create edges
    let domain_edge_type: domain::EdgeType = rule.edge_type.clone().into();
    let provenance = build_provenance(&pack.manifest.name, &rule.id, &pack.manifest.version);

    let mut edges: Vec<GraphEdge> = Vec::new();

    for artifact in &artifacts {
        // Extract attribute values from the artifact's JSON attrs
        let attrs: BTreeMap<String, Value> =
            serde_json::from_str(&artifact.attrs).unwrap_or_default();

        for target_node in &target_nodes {
            let mut all_matched = !rule.conditions.is_empty();

            for cond in &rule.conditions {
                let source_value = get_field_value(&attrs, &cond.field);
                let Some(source_value) = source_value else {
                    all_matched = false;
                    break;
                };

                let matched = match &cond.operator {
                    Operator::Equals => {
                        match_field_equals(&source_value, target_node, &cond.target_field, &files)
                    }
                    Operator::Contains => {
                        match_field_contains(&source_value, target_node, &cond.target_field, &files)
                    }
                    Operator::PathEquals => match_field_path_equals(
                        &source_value,
                        target_node,
                        &cond.target_field,
                        &files,
                    ),
                    Operator::FilenameEquals => match_field_filename_equals(
                        &source_value,
                        target_node,
                        &cond.target_field,
                        &files,
                    ),
                    Operator::Regex => {
                        match_field_regex(&source_value, target_node, &cond.target_field, &files)
                    }
                    Operator::TemporalProximity => false, // Not implemented yet
                };

                if !matched {
                    all_matched = false;
                    break;
                }
            }

            if all_matched {
                let edge_id = format!(
                    "rp:{}:{}:{}:{}",
                    &pack.manifest.name, &rule.id, &artifact.id, &target_node.id
                );

                // Map confidence string to numeric value
                let confidence = confidence_number(&rule.match_signals.confidence);

                edges.push(GraphEdge {
                    id: edge_id,
                    case_id: case_id.to_string(),
                    source_id: artifact.id.clone(),
                    target_id: target_node.id.clone(),
                    edge_type: domain_edge_type.clone(),
                    confidence: Some(confidence),
                    provenance: Some(provenance.clone()),
                    created_at: now.clone(),
                });
            }
        }
    }

    if edges.is_empty() {
        return Ok(0);
    }

    let count = edges.len() as u64;
    graph_repo.insert_edges_batch(&edges).map_err(|e| {
        RulePackError::Other(format!("insert rule pack edges for '{}': {e}", rule.id))
    })?;

    Ok(count)
}

// ── Data loading helpers ──

struct ArtifactRow {
    id: String,
    attrs: String,
}

fn load_artifacts_by_family(
    conn: &Connection,
    family: &str,
) -> Result<Vec<ArtifactRow>, RulePackError> {
    let repo = ArtifactRepo::new(conn);
    let rows = repo
        .find_by_family_raw(family)
        .map_err(|e| RulePackError::Other(format!("load artifacts by family '{family}': {e}")))?;
    let artifacts = rows
        .into_iter()
        .map(|(id, attrs)| ArtifactRow { id, attrs })
        .collect();
    Ok(artifacts)
}

struct NodeRow {
    id: String,
    label: String,
    summary: String,
}

fn load_nodes_by_type(
    conn: &Connection,
    case_id: &str,
    node_type: &NodeType,
) -> Result<Vec<NodeRow>, RulePackError> {
    let type_str = node_type_str(node_type);
    let repo = GraphRepo::new(conn);
    let rows = repo
        .find_nodes_by_type_for_case(case_id, type_str)
        .map_err(|e| RulePackError::Other(format!("load nodes by type '{type_str}': {e}")))?;
    let nodes = rows
        .into_iter()
        .map(|(id, label, summary)| NodeRow { id, label, summary })
        .collect();
    Ok(nodes)
}

struct FileEntryRow {
    id: String,
    path: String,
    name: String,
}

fn load_file_entries(conn: &Connection) -> Result<Vec<FileEntryRow>, RulePackError> {
    let mut stmt =
        conn.prepare("SELECT id, path, name FROM file_entries WHERE entry_type = 'file'")?;

    let rows = stmt.query_map([], |row| {
        Ok(FileEntryRow {
            id: row.get(0)?,
            path: row.get(1)?,
            name: row.get(2)?,
        })
    })?;

    let mut files = Vec::new();
    for row in rows {
        files.push(row?);
    }

    Ok(files)
}

// ── Value extraction ──

fn get_field_value(attrs: &BTreeMap<String, Value>, field: &str) -> Option<String> {
    attrs.get(field).and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Array(arr) => {
            // For array fields (e.g., attachments), join with spaces
            let parts: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(" "))
            }
        }
        _ => None,
    })
}

fn get_node_field_value(
    node: &NodeRow,
    files: &[FileEntryRow],
    target_field: &str,
) -> Option<String> {
    match target_field {
        "label" | "name" => {
            // Try to find a matching file entry for the node
            files
                .iter()
                .find(|f| f.id == node.id)
                .map(|f| f.name.clone())
                .or_else(|| Some(node.label.clone()))
        }
        "path" => files
            .iter()
            .find(|f| f.id == node.id)
            .map(|f| f.path.clone()),
        "summary" => Some(node.summary.clone()),
        _ => None,
    }
}

// ── Operator matchers ──

fn match_field_equals(
    source_value: &str,
    target_node: &NodeRow,
    target_field: &str,
    files: &[FileEntryRow],
) -> bool {
    let Some(target_value) = get_node_field_value(target_node, files, target_field) else {
        return false;
    };
    source_value.eq_ignore_ascii_case(&target_value)
}

fn match_field_contains(
    source_value: &str,
    target_node: &NodeRow,
    target_field: &str,
    files: &[FileEntryRow],
) -> bool {
    let Some(target_value) = get_node_field_value(target_node, files, target_field) else {
        return false;
    };
    target_value
        .to_ascii_lowercase()
        .contains(&source_value.to_ascii_lowercase())
}

fn match_field_path_equals(
    source_value: &str,
    target_node: &NodeRow,
    target_field: &str,
    files: &[FileEntryRow],
) -> bool {
    let Some(target_value) = get_node_field_value(target_node, files, target_field) else {
        return false;
    };
    let normalized_source = normalize_path(source_value);
    let normalized_target = normalize_path(&target_value);

    if normalized_source.is_empty() || normalized_target.is_empty() {
        return false;
    }

    // Exact match
    if normalized_source == normalized_target {
        return true;
    }

    // Suffix match: the artifact path may be a substring of the file path
    path_suffix_key(&normalized_source) == path_suffix_key(&normalized_target)
}

fn match_field_filename_equals(
    source_value: &str,
    target_node: &NodeRow,
    target_field: &str,
    files: &[FileEntryRow],
) -> bool {
    let Some(target_value) = get_node_field_value(target_node, files, target_field) else {
        return false;
    };

    // Extract basename from source value
    let source_name = normalize_path(source_value)
        .rsplit('/')
        .next()
        .unwrap_or(source_value)
        .to_string();
    let target_name = normalize_path(&target_value)
        .rsplit('/')
        .next()
        .unwrap_or(&target_value)
        .to_string();

    if source_name.is_empty() || target_name.is_empty() {
        return false;
    }

    source_name.eq_ignore_ascii_case(&target_name)
}

fn match_field_regex(
    source_value: &str,
    target_node: &NodeRow,
    target_field: &str,
    files: &[FileEntryRow],
) -> bool {
    let Some(target_value) = get_node_field_value(target_node, files, target_field) else {
        return false;
    };
    // The source_value is treated as the regex pattern
    match regex::Regex::new(source_value) {
        Ok(re) => re.is_match(&target_value),
        Err(_) => false,
    }
}

// ── Path utilities (mirror correlation_service.rs normalisation) ──

fn normalize_path(value: &str) -> String {
    let trimmed = value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches(|ch: char| matches!(ch, '(' | ')' | '[' | ']' | '<' | '>'));
    if trimmed.is_empty() {
        return String::new();
    }
    let mut normalized = trimmed.replace('\\', "/").to_ascii_lowercase();
    while normalized.contains("//") {
        normalized = normalized.replace("//", "/");
    }
    while normalized.ends_with('/') && normalized.len() > 3 {
        normalized.pop();
    }
    normalized
}

fn path_suffix_key(value: &str) -> String {
    let bytes = value.as_bytes();
    if value.len() >= 3 && bytes[1] == b':' && bytes[2] == b'/' {
        value[3..].to_string()
    } else {
        value.trim_start_matches('/').to_string()
    }
}

// ── Provenance ──

fn build_provenance(pack_id: &str, rule_id: &str, pack_version: &str) -> String {
    serde_json::json!({
        "pack_id": pack_id,
        "rule_id": rule_id,
        "pack_version": pack_version,
        "kind": "rule_pack",
    })
    .to_string()
}

fn confidence_number(confidence: &str) -> f64 {
    match confidence {
        "direct" => 1.0,
        "strong" => 0.9,
        "weak" => 0.5,
        "heuristic" => 0.3,
        _ => 0.5,
    }
}

// ── Incremental execution helpers ──

fn get_executed_rule_ids(
    conn: &Connection,
    case_id: &str,
    pack_id: &str,
) -> Result<std::collections::HashSet<String>, RulePackError> {
    let repo = GraphRepo::new(conn);
    let provenance_rows = repo
        .find_edges_with_provenance_by_case(case_id, "correlates_with")
        .map_err(|e| RulePackError::Other(format!("query executed rule ids: {e}")))?;

    let mut rule_ids = std::collections::HashSet::new();
    for provenance in provenance_rows {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&provenance) {
            if json.get("pack_id").and_then(|v| v.as_str()) == Some(pack_id) {
                if let Some(rule_id) = json.get("rule_id").and_then(|v| v.as_str()) {
                    rule_ids.insert(rule_id.to_string());
                }
            }
        }
    }

    Ok(rule_ids)
}

// ── Type string helpers (mirrors graph_repo.rs) ──

fn node_type_str(nt: &NodeType) -> &'static str {
    match nt {
        NodeType::File => "file",
        NodeType::Artifact => "artifact",
        NodeType::TimelineEvent => "timeline_event",
        NodeType::Entity => "entity",
        NodeType::Lead => "lead",
        NodeType::NotebookEntry => "notebook_entry",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule_pack::parser::parse_rule_pack;
    use crate::rule_pack::validator::validate_rule_pack;
    use chrono::Utc;
    use domain::{
        Artifact, ArtifactId, CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind,
        DataSourceProvenance, EdgeType, EntryType, FileEntry, FileEntryId, GraphNode,
    };
    use persistence_sqlite::repositories::{
        artifact_repo::ArtifactRepo, case_repo::CaseRepo, datasource_repo::DataSourceRepo,
        file_repo::FileRepo, graph_repo::GraphRepo,
    };
    use std::collections::BTreeMap;

    fn setup_case_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        persistence_sqlite::runner::run_all(&conn).unwrap();
        CaseRepo::new(&conn)
            .create(&CaseMeta {
                id: CaseId("case-1".to_string()),
                name: "Test Case".to_string(),
                number: None,
                examiner: None,
                notes: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .unwrap();
        DataSourceRepo::new(&conn)
            .insert(
                &CaseId("case-1".to_string()),
                &DataSource {
                    id: DataSourceId("ds-1".to_string()),
                    name: "test-source".to_string(),
                    kind: DataSourceKind::Raw,
                    source_path: std::path::PathBuf::from("C:/evidence/test.raw"),
                    imported_at: Utc::now(),
                    provenance: DataSourceProvenance::unknown(),
                },
            )
            .unwrap();
        conn
    }

    fn seed_test_graph(conn: &Connection) {
        let repo = GraphRepo::new(conn);
        let now = Utc::now().to_rfc3339();
        repo.insert_nodes_batch(&[
            GraphNode {
                id: "node-file-1".to_string(),
                case_id: "case-1".to_string(),
                node_type: NodeType::File,
                label: "cmd.exe".to_string(),
                summary: "C:/Windows/System32/cmd.exe".to_string(),
                tags: vec![],
                created_at: now.clone(),
            },
            GraphNode {
                id: "node-file-2".to_string(),
                case_id: "case-1".to_string(),
                node_type: NodeType::File,
                label: "payload.exe".to_string(),
                summary: "C:/Temp/payload.exe".to_string(),
                tags: vec![],
                created_at: now.clone(),
            },
            GraphNode {
                id: "node-file-3".to_string(),
                case_id: "case-1".to_string(),
                node_type: NodeType::File,
                label: "secrets.txt".to_string(),
                summary: "C:/Users/Admin/Desktop/secrets.txt".to_string(),
                tags: vec!["deleted".to_string()],
                created_at: now.clone(),
            },
        ])
        .unwrap();
    }

    fn seed_file_entries(conn: &Connection) {
        FileRepo::new(conn)
            .insert_batch(&[
                FileEntry {
                    id: FileEntryId("node-file-1".to_string()),
                    parent_id: None,
                    data_source_id: domain::DataSourceId("ds-1".to_string()),
                    path: "C:/Windows/System32/cmd.exe".to_string(),
                    name: "cmd.exe".to_string(),
                    entry_type: EntryType::File,
                    size: Some(1024),
                    ext: Some("exe".to_string()),
                    deleted: false,
                    hidden: false,
                    system: false,
                    encrypted: false,
                    created_at: None,
                    modified_at: None,
                    accessed_at: None,
                    changed_at: None,
                    hash_sha256: None,
                },
                FileEntry {
                    id: FileEntryId("node-file-2".to_string()),
                    parent_id: None,
                    data_source_id: domain::DataSourceId("ds-1".to_string()),
                    path: "C:/Temp/payload.exe".to_string(),
                    name: "payload.exe".to_string(),
                    entry_type: EntryType::File,
                    size: Some(2048),
                    ext: Some("exe".to_string()),
                    deleted: false,
                    hidden: false,
                    system: false,
                    encrypted: false,
                    created_at: None,
                    modified_at: None,
                    accessed_at: None,
                    changed_at: None,
                    hash_sha256: None,
                },
                FileEntry {
                    id: FileEntryId("node-file-3".to_string()),
                    parent_id: None,
                    data_source_id: domain::DataSourceId("ds-1".to_string()),
                    path: "C:/Users/Admin/Desktop/secrets.txt".to_string(),
                    name: "secrets.txt".to_string(),
                    entry_type: EntryType::File,
                    size: Some(512),
                    ext: Some("txt".to_string()),
                    deleted: true,
                    hidden: false,
                    system: false,
                    encrypted: false,
                    created_at: None,
                    modified_at: None,
                    accessed_at: None,
                    changed_at: None,
                    hash_sha256: None,
                },
            ])
            .unwrap();
    }

    fn seed_artifact(conn: &Connection, id: &str, family: &str, attrs: BTreeMap<String, Value>) {
        ArtifactRepo::new(conn)
            .insert_batch(
                &[Artifact {
                    id: ArtifactId(id.to_string()),
                    family: family.to_string(),
                    title: format!("{family} artifact"),
                    summary: "test fixture".to_string(),
                    source_object_id: Some(FileEntryId("node-artifact-source".to_string())),
                    extractor_id: Some(family.to_ascii_lowercase()),
                    extractor_version: Some("1.0.0".to_string()),
                    confidence: Some(0.9),
                    source_attribution: Some("fixture".to_string()),
                    created_at: Utc::now(),
                    attrs,
                }],
                "case-1",
                "ds-1",
            )
            .unwrap();
    }

    // ── Tests ──

    #[test]
    fn execute_lnk_path_match_rule() {
        let conn = setup_case_db();
        seed_test_graph(&conn);
        seed_file_entries(&conn);

        let mut attrs = BTreeMap::new();
        attrs.insert(
            "target_path".to_string(),
            Value::String("C:/Windows/System32/cmd.exe".to_string()),
        );
        seed_artifact(&conn, "artifact-lnk", "LNK", attrs);

        let toml = r#"
[manifest]
name = "lnk-test"
version = "1.0.0"
author = "test"
description = "Test LNK rule"
scope = ["test"]
min_product_version = "0.1.0"

[[rules]]
id = "lnk-path"
name = "LNK Path"
description = "Match LNK path"
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
        let pack = parse_rule_pack(toml).unwrap();
        let errors = validate_rule_pack(&pack);
        assert!(errors.is_empty(), "validation errors: {errors:?}");

        let count = execute_rule_pack(&conn, "case-1", &pack).unwrap();
        assert_eq!(count, 1);

        // Verify the edge was created
        let graph_repo = GraphRepo::new(&conn);
        let (_nodes, edges) = graph_repo
            .traverse(
                &["artifact-lnk".to_string()],
                &[EdgeType::CorrelatesWith],
                1,
                10,
            )
            .unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].edge_type, EdgeType::CorrelatesWith);
        assert_eq!(edges[0].source_id, "artifact-lnk");
        assert_eq!(edges[0].target_id, "node-file-1");

        // Verify provenance
        let provenance: Value =
            serde_json::from_str(edges[0].provenance.as_deref().unwrap_or("{}")).unwrap();
        assert_eq!(provenance["pack_id"], "lnk-test");
        assert_eq!(provenance["rule_id"], "lnk-path");
    }

    #[test]
    fn execute_prefetch_name_match_rule() {
        let conn = setup_case_db();
        seed_test_graph(&conn);
        seed_file_entries(&conn);

        let mut attrs = BTreeMap::new();
        attrs.insert(
            "executable".to_string(),
            Value::String("CMD.EXE".to_string()),
        );
        seed_artifact(&conn, "artifact-pf", "Prefetch", attrs);

        let toml = r#"
[manifest]
name = "pf-test"
version = "1.0.0"
author = "test"
description = "Test Prefetch rule"
scope = ["test"]
min_product_version = "0.1.0"

[[rules]]
id = "pf-name"
name = "Prefetch Name"
description = "Match Prefetch executable name"
source_type = "artifact"
source_family = "Prefetch"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "executable"
operator = "filename_equals"
target_field = "name"

[rules.match_signals]
confidence = "strong"
"#;
        let pack = parse_rule_pack(toml).unwrap();
        let errors = validate_rule_pack(&pack);
        assert!(errors.is_empty(), "validation errors: {errors:?}");

        let count = execute_rule_pack(&conn, "case-1", &pack).unwrap();
        assert_eq!(count, 1);

        let graph_repo = GraphRepo::new(&conn);
        let (_nodes, edges) = graph_repo
            .traverse(
                &["artifact-pf".to_string()],
                &[EdgeType::CorrelatesWith],
                1,
                10,
            )
            .unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].target_id, "node-file-1");
        assert!(edges[0].confidence.unwrap_or(0.0) > 0.8); // strong = 0.9
    }

    #[test]
    fn execute_pack_with_no_matching_artifacts() {
        let conn = setup_case_db();
        seed_test_graph(&conn);
        seed_file_entries(&conn);

        // No BrowserDownload artifacts seeded

        let toml = r#"
[manifest]
name = "no-match-test"
version = "1.0.0"
author = "test"
description = "No matching artifacts"
scope = ["test"]
min_product_version = "0.1.0"

[[rules]]
id = "bd-path"
name = "BrowserDownload Path"
description = "Match download path"
source_type = "artifact"
source_family = "BrowserDownload"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "targetPath"
operator = "path_equals"
target_field = "path"

[rules.match_signals]
confidence = "direct"
"#;
        let pack = parse_rule_pack(toml).unwrap();
        let count = execute_rule_pack(&conn, "case-1", &pack).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn execute_incremental_skips_executed_rules() {
        let conn = setup_case_db();
        seed_test_graph(&conn);
        seed_file_entries(&conn);

        let mut attrs = BTreeMap::new();
        attrs.insert(
            "target_path".to_string(),
            Value::String("C:/Windows/System32/cmd.exe".to_string()),
        );
        seed_artifact(&conn, "artifact-lnk-1", "LNK", attrs.clone());
        seed_artifact(&conn, "artifact-lnk-2", "LNK", attrs);

        let toml = r#"
[manifest]
name = "incr-test"
version = "1.0.0"
author = "test"
description = "Incremental test"
scope = ["test"]
min_product_version = "0.1.0"

[[rules]]
id = "lnk-path"
name = "LNK Path"
description = "Match LNK path"
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
        let pack = parse_rule_pack(toml).unwrap();

        // First run creates edges
        let count1 = execute_rule_pack(&conn, "case-1", &pack).unwrap();
        assert!(count1 > 0, "first run should create edges");

        // Second run with incremental should skip
        let count2 = execute_rule_pack_incremental(&conn, "case-1", &pack).unwrap();
        assert_eq!(
            count2, 0,
            "incremental run should skip already-executed rules"
        );
    }

    #[test]
    fn execute_v2_standard_pack() {
        let conn = setup_case_db();
        seed_test_graph(&conn);
        seed_file_entries(&conn);

        // Seed LNK artifact
        let mut attrs_lnk = BTreeMap::new();
        attrs_lnk.insert(
            "target_path".to_string(),
            Value::String("C:/Windows/System32/cmd.exe".to_string()),
        );
        seed_artifact(&conn, "artifact-lnk-v2", "LNK", attrs_lnk);

        // Seed Prefetch artifact
        let mut attrs_pf = BTreeMap::new();
        attrs_pf.insert(
            "executable".to_string(),
            Value::String("cmd.exe".to_string()),
        );
        seed_artifact(&conn, "artifact-pf-v2", "Prefetch", attrs_pf);

        // Seed RecycleBin artifact
        let mut attrs_rb = BTreeMap::new();
        attrs_rb.insert(
            "original_path".to_string(),
            Value::String("C:/Users/Admin/Desktop/secrets.txt".to_string()),
        );
        seed_artifact(&conn, "artifact-rb-v2", "RecycleBin", attrs_rb);

        // Load and execute the v2-standard pack from the public parser constant
        let pack = parse_rule_pack(crate::rule_pack::parser::V2_STANDARD_TOML).unwrap();

        let count = execute_rule_pack(&conn, "case-1", &pack).unwrap();
        // At minimum, the LNK rule should match
        assert!(count > 0, "should create at least one edge");

        // Verify edges exist
        let graph_repo = GraphRepo::new(&conn);
        let (_nodes, edges) = graph_repo
            .traverse(
                &["artifact-lnk-v2".to_string()],
                &[EdgeType::CorrelatesWith],
                1,
                10,
            )
            .unwrap();
        assert!(!edges.is_empty());
    }

    #[test]
    fn execute_rule_pack_returns_zero_for_unknown_source_types() {
        let conn = setup_case_db();

        let toml = r#"
[manifest]
name = "future-test"
version = "1.0.0"
author = "test"
description = "Future source type"
scope = ["test"]
min_product_version = "0.1.0"

[[rules]]
id = "temporal-rule"
name = "Temporal"
description = "Future temporal rule"
source_type = "timeline_event"
source_family = "TimelineEvent"
target_type = "file"
edge_type = "correlates_with"

[[rules.conditions]]
field = "timestamp"
operator = "temporal_proximity"
target_field = "created_at"

[rules.match_signals]
confidence = "direct"
"#;
        let pack = parse_rule_pack(toml).unwrap();
        let count = execute_rule_pack(&conn, "case-1", &pack).unwrap();
        // TimelineEvent rules are not yet implemented
        assert_eq!(count, 0);
    }
}
