#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use domain::{
    Artifact, ArtifactId, CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind,
    DataSourceProvenance, EdgeType, EntryType, FileEntry, FileEntryId, GraphEdge, GraphNode,
    NodeType,
};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, case_repo::CaseRepo, datasource_repo::DataSourceRepo,
    file_repo::FileRepo, graph_repo::GraphRepo,
};
use rusqlite::Connection;
use serde_json::Value;

pub const CASE_ID: &str = "case-1";
pub const SOURCE_ID: &str = "ds-1";

pub fn case_db() -> Connection {
    let conn = persistence_sqlite::connection::open_in_memory().expect("open test database");
    persistence_sqlite::runner::run_all(&conn).expect("run migrations");
    seed_case(&conn, CASE_ID, "Test Case");
    seed_source(&conn);
    conn
}

pub fn seed_case(conn: &Connection, case_id: &str, name: &str) {
    CaseRepo::new(conn)
        .create(&CaseMeta {
            id: CaseId(case_id.to_string()),
            name: name.to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .expect("insert case");
}

fn seed_source(conn: &Connection) {
    DataSourceRepo::new(conn)
        .insert(
            &CaseId(CASE_ID.to_string()),
            &DataSource {
                id: DataSourceId(SOURCE_ID.to_string()),
                name: "test-source".to_string(),
                kind: DataSourceKind::LogicalDirectory,
                source_path: PathBuf::from("C:/evidence"),
                imported_at: Utc::now(),
                provenance: DataSourceProvenance::unknown(),
            },
        )
        .expect("insert data source");
}

pub fn insert_artifact(
    conn: &Connection,
    id: &str,
    family: &str,
    title: &str,
    summary: &str,
    attrs: BTreeMap<String, Value>,
) {
    ArtifactRepo::new(conn)
        .insert_batch(
            &[Artifact {
                id: ArtifactId(id.to_string()),
                family: family.to_string(),
                title: title.to_string(),
                summary: summary.to_string(),
                source_object_id: Some(FileEntryId(format!("source-{id}"))),
                extractor_id: Some(family.to_ascii_lowercase()),
                extractor_version: Some("1.0.0".to_string()),
                confidence: Some(0.9),
                source_attribution: Some("fixture".to_string()),
                created_at: Utc::now(),
                attrs,
            }],
            CASE_ID,
            SOURCE_ID,
        )
        .expect("insert artifact");
}

pub fn insert_graph_node(
    conn: &Connection,
    id: &str,
    node_type: NodeType,
    label: &str,
    tags: &[&str],
) {
    GraphRepo::new(conn)
        .insert_nodes_batch(&[GraphNode {
            id: id.to_string(),
            case_id: CASE_ID.to_string(),
            node_type,
            label: label.to_string(),
            summary: String::new(),
            tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
            created_at: Utc::now().to_rfc3339(),
        }])
        .expect("insert graph node");
}

pub fn insert_graph_edge(
    conn: &Connection,
    id: &str,
    source_id: &str,
    target_id: &str,
    edge_type: EdgeType,
) {
    GraphRepo::new(conn)
        .insert_edges_batch(&[GraphEdge {
            id: id.to_string(),
            case_id: CASE_ID.to_string(),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            edge_type,
            confidence: None,
            provenance: Some("test".to_string()),
            created_at: Utc::now().to_rfc3339(),
        }])
        .expect("insert graph edge");
}

pub fn seed_file(conn: &Connection, id: &str, path: &str, name: &str) {
    FileRepo::new(conn)
        .insert_batch(&[FileEntry {
            id: FileEntryId(id.to_string()),
            parent_id: None,
            data_source_id: DataSourceId(SOURCE_ID.to_string()),
            path: path.to_string(),
            name: name.to_string(),
            entry_type: EntryType::File,
            size: Some(128),
            ext: Path::new(name)
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_string),
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            read_only: false,
            archive: false,
            unix_mode: None,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        }])
        .expect("insert file");
}

pub fn create_case_file(
    directory: &Path,
    case_id: &str,
    entities: &[(&str, &str, &str)],
) -> PathBuf {
    let path = directory.join(format!("{case_id}.db"));
    let conn = persistence_sqlite::connection::open_or_create(&path).expect("open case file");
    persistence_sqlite::runner::run_all(&conn).expect("run case migrations");
    seed_case(&conn, case_id, case_id);
    for (entity_id, entity_type, canonical_value) in entities {
        conn.execute(
            "INSERT INTO resolved_entities
             (id, case_id, entity_type, canonical_value, source_count, confidence, attributes_json)
             VALUES (?1, ?2, ?3, ?4, 1, 0.85, '[]')",
            rusqlite::params![entity_id, case_id, entity_type, canonical_value],
        )
        .expect("insert resolved entity");
    }
    drop(conn);
    path
}
