use chrono::Utc;
use domain::{DataSourceId, EdgeType, GraphEdge, GraphNode, NodeType};
use persistence_sqlite::{repositories::graph_repo::GraphRepo, DbResult};
use rusqlite::Connection;

const GRAPH_QUERY_BATCH: u32 = 5000;
const GRAPH_WRITE_CHUNK: usize = 2000;
type NodeRow = (String, Option<String>, String, String);

pub fn populate_file_graph_for_data_source(
    conn: &Connection,
    data_source_id: &DataSourceId,
) -> DbResult<()> {
    let case_id: String = conn.query_row(
        "SELECT case_id FROM data_sources WHERE id = ?1",
        rusqlite::params![data_source_id.0],
        |row| row.get(0),
    )?;
    let graph_repo = GraphRepo::new(conn);
    let created_at = Utc::now().to_rfc3339();
    insert_file_nodes(conn, &graph_repo, data_source_id, &case_id, &created_at)?;
    insert_contains_edges(conn, &graph_repo, data_source_id, &case_id, &created_at)
}

fn insert_file_nodes(
    conn: &Connection,
    graph_repo: &GraphRepo<'_>,
    data_source_id: &DataSourceId,
    case_id: &str,
    created_at: &str,
) -> DbResult<()> {
    let mut offset = 0u64;
    loop {
        let rows = read_node_rows(conn, data_source_id, offset)?;
        if rows.is_empty() {
            return Ok(());
        }
        let row_count = rows.len() as u64;
        let nodes = rows
            .into_iter()
            .filter(|(_, parent_id, name, _)| !(name.is_empty() && parent_id.is_none()))
            .map(|(id, _, name, path)| GraphNode {
                id,
                case_id: case_id.to_string(),
                node_type: NodeType::File,
                label: name,
                summary: path,
                tags: Vec::new(),
                created_at: created_at.to_string(),
            })
            .collect::<Vec<_>>();
        for chunk in nodes.chunks(GRAPH_WRITE_CHUNK) {
            graph_repo.insert_nodes_batch(chunk)?;
        }
        offset += row_count;
    }
}

fn read_node_rows(
    conn: &Connection,
    data_source_id: &DataSourceId,
    offset: u64,
) -> DbResult<Vec<NodeRow>> {
    let mut statement = conn.prepare(
        "SELECT id, parent_id, name, path FROM file_entries
         WHERE data_source_id = ?1 LIMIT ?2 OFFSET ?3",
    )?;
    let rows = statement
        .query_map(
            rusqlite::params![data_source_id.0, GRAPH_QUERY_BATCH, offset],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn insert_contains_edges(
    conn: &Connection,
    graph_repo: &GraphRepo<'_>,
    data_source_id: &DataSourceId,
    case_id: &str,
    created_at: &str,
) -> DbResult<()> {
    let mut offset = 0u64;
    loop {
        let rows = read_edge_rows(conn, data_source_id, offset)?;
        if rows.is_empty() {
            return Ok(());
        }
        let row_count = rows.len() as u64;
        let edges = rows
            .into_iter()
            .filter_map(|(id, parent_id)| {
                parent_id.map(|parent_id| GraphEdge {
                    id: format!("contains:{parent_id}:{id}"),
                    case_id: case_id.to_string(),
                    source_id: parent_id,
                    target_id: id,
                    edge_type: EdgeType::Contains,
                    confidence: None,
                    provenance: None,
                    created_at: created_at.to_string(),
                })
            })
            .collect::<Vec<_>>();
        for chunk in edges.chunks(GRAPH_WRITE_CHUNK) {
            graph_repo.insert_edges_batch(chunk)?;
        }
        offset += row_count;
    }
}

fn read_edge_rows(
    conn: &Connection,
    data_source_id: &DataSourceId,
    offset: u64,
) -> DbResult<Vec<(String, Option<String>)>> {
    let mut statement = conn.prepare(
        "SELECT id, parent_id FROM file_entries
         WHERE data_source_id = ?1 LIMIT ?2 OFFSET ?3",
    )?;
    let rows = statement
        .query_map(
            rusqlite::params![data_source_id.0, GRAPH_QUERY_BATCH, offset],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
