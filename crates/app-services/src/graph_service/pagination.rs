use std::{
    cmp::Ordering,
    collections::{BinaryHeap, VecDeque},
    path::Path,
};

use domain::DataSourceId;
use persistence_sqlite::repositories::graph_repo::{GraphNodePageCursor, GraphRepo};
use rusqlite::Connection;
use transport::dto::{GraphNodeDto, ListGraphNodesRequest};

use crate::source_db;

use super::{query::node_to_dto, source_aggregation::scope_graph_node, GraphServiceError};

const GRAPH_NODE_MERGE_BATCH_SIZE: u32 = 128;

pub fn list_graph_nodes_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &str,
    request: ListGraphNodesRequest,
) -> Result<Vec<GraphNodeDto>, GraphServiceError> {
    let sources = source_db::open_ready_source_connections(
        case_conn,
        case_root,
        &domain::CaseId(case_id.to_string()),
    )?;
    merge_graph_node_page(sources, case_id, request)
}

pub fn list_graph_nodes(
    conn: &Connection,
    case_id: &str,
    request: ListGraphNodesRequest,
) -> Result<Vec<GraphNodeDto>, GraphServiceError> {
    let nodes = GraphRepo::new(conn)
        .list_nodes_for_case(case_id, request.limit.clamp(1, 500), request.offset)
        .map_err(|error| GraphServiceError::Other(format!("graph node list query: {error}")))?;
    Ok(nodes.into_iter().map(node_to_dto).collect())
}

struct GraphNodeSourceCursor {
    data_source_id: DataSourceId,
    connection: Connection,
    after: Option<GraphNodePageCursor>,
    buffered: VecDeque<GraphNodeDto>,
    exhausted: bool,
}

impl GraphNodeSourceCursor {
    fn new(data_source_id: DataSourceId, connection: Connection) -> Self {
        Self {
            data_source_id,
            connection,
            after: None,
            buffered: VecDeque::new(),
            exhausted: false,
        }
    }

    fn next_node(
        &mut self,
        case_id: &str,
        batch_size: u32,
    ) -> Result<Option<GraphNodeDto>, GraphServiceError> {
        if self.buffered.is_empty() && !self.exhausted {
            self.refill(case_id, batch_size)?;
        }
        Ok(self
            .buffered
            .pop_front()
            .map(|node| scope_graph_node(node, &self.data_source_id)))
    }

    fn refill(&mut self, case_id: &str, batch_size: u32) -> Result<(), GraphServiceError> {
        let nodes = GraphRepo::new(&self.connection)
            .list_nodes_for_case_after(case_id, batch_size, self.after.as_ref())
            .map_err(|error| {
                GraphServiceError::Other(format!("graph node keyset query: {error}"))
            })?;
        self.exhausted = nodes.len() < batch_size as usize;
        self.after = nodes.last().map(GraphNodePageCursor::from);
        self.buffered = nodes.into_iter().map(node_to_dto).collect();
        Ok(())
    }
}

struct GraphNodeMergeEntry {
    cursor_index: usize,
    node: GraphNodeDto,
}

impl PartialEq for GraphNodeMergeEntry {
    fn eq(&self, other: &Self) -> bool {
        self.cursor_index == other.cursor_index
            && self.node.created_at == other.node.created_at
            && self.node.id == other.node.id
    }
}

impl Eq for GraphNodeMergeEntry {}

impl PartialOrd for GraphNodeMergeEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GraphNodeMergeEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.node
            .created_at
            .cmp(&other.node.created_at)
            .then_with(|| other.node.id.cmp(&self.node.id))
            .then_with(|| other.cursor_index.cmp(&self.cursor_index))
    }
}

fn merge_graph_node_page(
    sources: Vec<(DataSourceId, Connection)>,
    case_id: &str,
    request: ListGraphNodesRequest,
) -> Result<Vec<GraphNodeDto>, GraphServiceError> {
    let limit = request.limit.clamp(1, 500);
    let page_end = u64::from(request.offset)
        .checked_add(u64::from(limit))
        .ok_or_else(|| GraphServiceError::InvalidInput("graph page range overflow".to_string()))?;
    let batch_size = u32::try_from(page_end.min(u64::from(GRAPH_NODE_MERGE_BATCH_SIZE)))
        .map_err(|_| GraphServiceError::InvalidInput("graph merge batch overflow".to_string()))?;
    let mut cursors = sources
        .into_iter()
        .map(|(source_id, connection)| GraphNodeSourceCursor::new(source_id, connection))
        .collect::<Vec<_>>();
    let mut heap = seed_heap(&mut cursors, case_id, batch_size)?;
    drain_page(
        &mut cursors,
        &mut heap,
        case_id,
        request.offset,
        page_end,
        batch_size,
    )
}

fn seed_heap(
    cursors: &mut [GraphNodeSourceCursor],
    case_id: &str,
    batch_size: u32,
) -> Result<BinaryHeap<GraphNodeMergeEntry>, GraphServiceError> {
    let mut heap = BinaryHeap::with_capacity(cursors.len());
    for (cursor_index, cursor) in cursors.iter_mut().enumerate() {
        if let Some(node) = cursor.next_node(case_id, batch_size)? {
            heap.push(GraphNodeMergeEntry { cursor_index, node });
        }
    }
    Ok(heap)
}

fn drain_page(
    cursors: &mut [GraphNodeSourceCursor],
    heap: &mut BinaryHeap<GraphNodeMergeEntry>,
    case_id: &str,
    offset: u32,
    page_end: u64,
    batch_size: u32,
) -> Result<Vec<GraphNodeDto>, GraphServiceError> {
    let mut position = 0_u64;
    let mut page = Vec::with_capacity((page_end - u64::from(offset)) as usize);
    while position < page_end {
        let Some(entry) = heap.pop() else {
            break;
        };
        if position >= u64::from(offset) {
            page.push(entry.node);
        }
        position += 1;
        if position < page_end {
            if let Some(node) = cursors[entry.cursor_index].next_node(case_id, batch_size)? {
                heap.push(GraphNodeMergeEntry {
                    cursor_index: entry.cursor_index,
                    node,
                });
            }
        }
    }
    Ok(page)
}
