use domain::{EdgeType, GraphEdge, GraphNode, NodeType};

pub(super) const NODE_COLUMNS: &str = "id, case_id, node_type, label, summary, tags, created_at";
pub(super) const EDGE_COLUMNS: &str =
    "id, case_id, source_id, target_id, edge_type, confidence, provenance, created_at";

pub(super) fn node_type_str(node_type: &NodeType) -> &'static str {
    match node_type {
        NodeType::File => "file",
        NodeType::Artifact => "artifact",
        NodeType::TimelineEvent => "timeline_event",
        NodeType::Entity => "entity",
        NodeType::Lead => "lead",
        NodeType::NotebookEntry => "notebook_entry",
    }
}

fn parse_node_type(value: &str) -> NodeType {
    match value {
        "file" => NodeType::File,
        "artifact" => NodeType::Artifact,
        "timeline_event" => NodeType::TimelineEvent,
        "entity" => NodeType::Entity,
        "lead" => NodeType::Lead,
        "notebook_entry" => NodeType::NotebookEntry,
        _ => NodeType::Entity,
    }
}

pub(super) fn edge_type_str(edge_type: &EdgeType) -> &'static str {
    match edge_type {
        EdgeType::Contains => "contains",
        EdgeType::References => "references",
        EdgeType::CorrelatesWith => "correlates_with",
        EdgeType::DerivesFrom => "derives_from",
        EdgeType::Precedes => "precedes",
        EdgeType::Cites => "cites",
        EdgeType::Annotates => "annotates",
    }
}

pub(super) fn parse_edge_type(value: &str) -> EdgeType {
    match value {
        "contains" => EdgeType::Contains,
        "references" => EdgeType::References,
        "correlates_with" => EdgeType::CorrelatesWith,
        "derives_from" => EdgeType::DerivesFrom,
        "precedes" => EdgeType::Precedes,
        "cites" => EdgeType::Cites,
        "annotates" => EdgeType::Annotates,
        _ => EdgeType::References,
    }
}

pub(super) fn row_to_node(row: &rusqlite::Row<'_>) -> rusqlite::Result<GraphNode> {
    let tags: Vec<String> = serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default();
    Ok(GraphNode {
        id: row.get(0)?,
        case_id: row.get(1)?,
        node_type: parse_node_type(&row.get::<_, String>(2)?),
        label: row.get(3)?,
        summary: row.get(4)?,
        tags,
        created_at: row.get(6)?,
    })
}

pub(super) fn row_to_edge(row: &rusqlite::Row<'_>) -> rusqlite::Result<GraphEdge> {
    Ok(GraphEdge {
        id: row.get(0)?,
        case_id: row.get(1)?,
        source_id: row.get(2)?,
        target_id: row.get(3)?,
        edge_type: parse_edge_type(&row.get::<_, String>(4)?),
        confidence: row.get(5)?,
        provenance: row.get(6)?,
        created_at: row.get(7)?,
    })
}

pub(super) fn row_to_edge_node_pair(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(GraphEdge, GraphNode)> {
    let edge = row_to_edge(row)?;
    let node = GraphNode {
        id: row.get(8)?,
        case_id: row.get(9)?,
        node_type: parse_node_type(&row.get::<_, String>(10)?),
        label: row.get(11)?,
        summary: row.get(12)?,
        tags: serde_json::from_str(&row.get::<_, String>(13)?).unwrap_or_default(),
        created_at: row.get(14)?,
    };
    Ok((edge, node))
}
