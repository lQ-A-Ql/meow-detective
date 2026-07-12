use domain::EvidenceCitation;
use persistence_sqlite::repositories::notebook_repo::NotebookRepo;
use rusqlite::Connection;
use transport::dto::{EvidenceCitationDto, GraphNodeTypeDto};
use uuid::Uuid;

use super::dto_conversion::{citation_to_dto, node_type_from_dto};
use super::NotebookError;

/// Add an evidence citation linking a notebook entry to a graph node.
pub fn add_citation(
    conn: &Connection,
    entry_id: &str,
    target_node_type: &GraphNodeTypeDto,
    target_node_id: &str,
    display_label: &str,
    snippet: Option<&str>,
) -> Result<EvidenceCitationDto, NotebookError> {
    let citation = EvidenceCitation {
        id: Uuid::new_v4().to_string(),
        entry_id: entry_id.to_string(),
        target_node_type: node_type_from_dto(target_node_type),
        target_node_id: target_node_id.to_string(),
        display_label: display_label.to_string(),
        snippet: snippet.map(str::to_string),
        cited_at: chrono::Utc::now().to_rfc3339(),
    };

    NotebookRepo::new(conn).add_citation(&citation)?;

    Ok(citation_to_dto(&citation))
}
