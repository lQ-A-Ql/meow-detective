use persistence_sqlite::repositories::{artifact_repo::ArtifactRepo, graph_repo::GraphRepo};
use rusqlite::Connection;

use super::super::error::RulePackError;

#[derive(Debug)]
pub(super) struct ArtifactRow {
    pub id: String,
    pub attrs: String,
}

#[derive(Debug)]
pub(super) struct NodeRow {
    pub id: String,
    pub label: String,
    pub summary: String,
}

#[derive(Debug)]
pub(super) struct FileEntryRow {
    pub id: String,
    pub path: String,
    pub name: String,
}

pub(super) fn artifacts_by_family(
    conn: &Connection,
    family: &str,
) -> Result<Vec<ArtifactRow>, RulePackError> {
    let mut rows: Vec<ArtifactRow> = ArtifactRepo::new(conn)
        .find_by_family_raw(family)
        .map_err(|error| {
            RulePackError::Other(format!("load artifacts by family '{family}': {error}"))
        })?
        .into_iter()
        .map(|(id, attrs)| ArtifactRow { id, attrs })
        .collect();
    rows.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(rows)
}

pub(super) fn nodes_by_type(
    conn: &Connection,
    case_id: &str,
    node_type: &domain::NodeType,
) -> Result<Vec<NodeRow>, RulePackError> {
    let type_name = node_type_name(node_type);
    let mut rows: Vec<NodeRow> = GraphRepo::new(conn)
        .find_nodes_by_type_for_case(case_id, type_name)
        .map_err(|error| {
            RulePackError::Other(format!("load nodes by type '{type_name}': {error}"))
        })?
        .into_iter()
        .map(|(id, label, summary)| NodeRow { id, label, summary })
        .collect();
    rows.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(rows)
}

pub(super) fn file_entries(conn: &Connection) -> Result<Vec<FileEntryRow>, RulePackError> {
    let mut statement = conn.prepare(
        "SELECT id, path, name FROM file_entries
         WHERE entry_type = 'file' ORDER BY id",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok(FileEntryRow {
                id: row.get(0)?,
                path: row.get(1)?,
                name: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(RulePackError::from)?;
    Ok(rows)
}

fn node_type_name(node_type: &domain::NodeType) -> &'static str {
    match node_type {
        domain::NodeType::File => "file",
        domain::NodeType::Artifact => "artifact",
        domain::NodeType::TimelineEvent => "timeline_event",
        domain::NodeType::Entity => "entity",
        domain::NodeType::Lead => "lead",
        domain::NodeType::NotebookEntry => "notebook_entry",
    }
}
