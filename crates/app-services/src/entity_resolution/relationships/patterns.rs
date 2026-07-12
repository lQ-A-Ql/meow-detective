use rusqlite::Connection;

use crate::entity_resolution::EntityResolutionError;

pub(super) type RelationshipRow = (String, String, String, String);

pub(super) fn communicates_with(
    conn: &Connection,
    case_id: &str,
) -> Result<Vec<RelationshipRow>, EntityResolutionError> {
    query_rows(
        conn,
        case_id,
        "SELECT e1.id, e2.id,
                GROUP_CONCAT(DISTINCT ge1.id), GROUP_CONCAT(DISTINCT ge2.id)
         FROM graph_nodes e1
         JOIN graph_edges ge1 ON ge1.source_id = e1.id
            AND ge1.edge_type = 'correlates_with'
         JOIN graph_nodes art ON art.id = ge1.target_id AND art.node_type = 'artifact'
         JOIN graph_edges ge2 ON ge2.target_id = art.id
            AND ge2.edge_type = 'correlates_with' AND ge2.source_id != e1.id
         JOIN graph_nodes e2 ON e2.id = ge2.source_id AND e2.node_type = 'entity'
         WHERE e1.case_id = ?1 AND e1.node_type = 'entity'
           AND e1.tags LIKE '%\"person\"%' AND e2.tags LIKE '%\"person\"%'
           AND e1.id < e2.id
           AND (art.tags LIKE '%\"EmailMessage\"%' OR LOWER(art.label) LIKE '%email%')
         GROUP BY e1.id, e2.id ORDER BY e1.id, e2.id",
    )
}

pub(super) fn ownership(
    conn: &Connection,
    case_id: &str,
) -> Result<Vec<RelationshipRow>, EntityResolutionError> {
    device_file_pattern(conn, case_id, "Registry", "registry")
}

pub(super) fn logged_into(
    conn: &Connection,
    case_id: &str,
) -> Result<Vec<RelationshipRow>, EntityResolutionError> {
    device_file_pattern(conn, case_id, "wtmp", "wtmp")
}

pub(super) fn executed(
    conn: &Connection,
    case_id: &str,
) -> Result<Vec<RelationshipRow>, EntityResolutionError> {
    query_rows(
        conn,
        case_id,
        "SELECT e1.id, f.id,
                GROUP_CONCAT(DISTINCT ge1.id), GROUP_CONCAT(DISTINCT ge2.id)
         FROM graph_nodes e1
         JOIN graph_edges ge1 ON ge1.source_id = e1.id
            AND ge1.edge_type = 'derives_from'
         JOIN graph_nodes art ON art.id = ge1.target_id AND art.node_type = 'artifact'
         JOIN graph_edges ge2 ON ge2.source_id = art.id AND ge2.edge_type = 'references'
         JOIN graph_nodes f ON f.id = ge2.target_id AND f.node_type = 'file'
         WHERE e1.case_id = ?1 AND e1.node_type = 'entity'
           AND e1.tags LIKE '%\"person\"%'
           AND (art.tags LIKE '%\"Prefetch\"%' OR LOWER(art.label) LIKE '%prefetch%')
         GROUP BY e1.id, f.id ORDER BY e1.id, f.id",
    )
}

fn device_file_pattern(
    conn: &Connection,
    case_id: &str,
    artifact_tag: &str,
    label_fragment: &str,
) -> Result<Vec<RelationshipRow>, EntityResolutionError> {
    let sql = format!(
        "SELECT e1.id, e2.id,
                GROUP_CONCAT(DISTINCT ge1.id),
                GROUP_CONCAT(DISTINCT ge2.id || ',' || ge3.id)
         FROM graph_nodes e1
         JOIN graph_edges ge1 ON ge1.source_id = e1.id AND ge1.edge_type = 'derives_from'
         JOIN graph_nodes art ON art.id = ge1.target_id AND art.node_type = 'artifact'
         JOIN graph_edges ge2 ON ge2.source_id = art.id AND ge2.edge_type = 'references'
         JOIN graph_nodes f ON f.id = ge2.target_id AND f.node_type = 'file'
         JOIN graph_edges ge3 ON
            (ge3.source_id = f.id AND ge3.target_id = e2.id)
            OR (ge3.target_id = f.id AND ge3.source_id = e2.id)
         JOIN graph_nodes e2 ON e2.id =
            CASE WHEN ge3.source_id = f.id THEN ge3.target_id ELSE ge3.source_id END
         WHERE e1.case_id = ?1 AND e1.node_type = 'entity'
           AND e1.tags LIKE '%\"person\"%' AND e2.node_type = 'entity'
           AND e2.tags LIKE '%\"device\"%'
           AND (art.tags LIKE '%\"{artifact_tag}\"%' OR LOWER(art.label) LIKE '%{label_fragment}%')
           AND e1.id != e2.id
         GROUP BY e1.id, e2.id ORDER BY e1.id, e2.id"
    );
    query_rows(conn, case_id, &sql)
}

fn query_rows(
    conn: &Connection,
    case_id: &str,
    sql: &str,
) -> Result<Vec<RelationshipRow>, EntityResolutionError> {
    let mut statement = conn.prepare(sql)?;
    let rows = statement
        .query_map(rusqlite::params![case_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(EntityResolutionError::from)?;
    Ok(rows)
}
