use super::{require_case, DbError, DbResult, SourceRevisionRef, SourceRevisionRepo};
use std::collections::{BTreeMap, VecDeque};

const MAX_NODES: usize = 1024;
pub(super) const MAX_EDGES: usize = 4096;
const MAX_DEPTH: usize = 64;

type Graph = BTreeMap<String, Vec<String>>;

pub(super) fn validate(
    repo: &SourceRevisionRepo<'_>,
    case_id: &str,
    root: &str,
    proposed_inputs: Option<&[SourceRevisionRef]>,
    require_inputs: bool,
) -> DbResult<()> {
    validated_graph(repo, case_id, root, proposed_inputs, require_inputs).map(|_| ())
}

pub(super) fn snapshot(
    repo: &SourceRevisionRepo<'_>,
    reference: &SourceRevisionRef,
) -> DbResult<Vec<SourceRevisionRef>> {
    repo.require_current_revision(reference)?;
    let graph = validated_graph(repo, &reference.case_id, &reference.source_id, None, false)?;
    graph
        .keys()
        .map(|source| {
            repo.current(&reference.case_id, source)
                .map(|(reference, _)| reference)
        })
        .collect()
}

fn validated_graph(
    repo: &SourceRevisionRepo<'_>,
    case_id: &str,
    root: &str,
    proposed_inputs: Option<&[SourceRevisionRef]>,
    require_inputs: bool,
) -> DbResult<Graph> {
    require_case(repo.conn, case_id, root)?;
    let mut graph = Graph::new();
    let mut pending = VecDeque::from([root.to_owned()]);
    let mut edges = 0usize;
    while let Some(source) = pending.pop_front() {
        if graph.contains_key(&source) {
            continue;
        }
        if graph.len() >= MAX_NODES {
            return Err(limit_error());
        }
        let inputs = match proposed_inputs.filter(|_| source == root) {
            Some(inputs) => inputs.to_vec(),
            None => bounded_inputs(repo, case_id, &source, MAX_EDGES - edges)?,
        };
        if source == root && require_inputs && inputs.is_empty() {
            return Err(DbError::System("structure view inputs are missing".into()));
        }
        edges += inputs.len();
        if edges > MAX_EDGES {
            return Err(limit_error());
        }
        let mut parents = Vec::with_capacity(inputs.len());
        for input in inputs {
            if input.case_id != case_id {
                return Err(DbError::System("structure dependency case mismatch".into()));
            }
            repo.require_current_revision(&input)?;
            pending.push_back(input.source_id.clone());
            parents.push(input.source_id);
        }
        graph.insert(source, parents);
    }
    validate_topology(&graph, root)?;
    Ok(graph)
}

fn bounded_inputs(
    repo: &SourceRevisionRepo<'_>,
    case_id: &str,
    source: &str,
    remaining: usize,
) -> DbResult<Vec<SourceRevisionRef>> {
    let mut statement = repo.conn.prepare(
        "SELECT parent_source_id, parent_revision FROM structure_view_inputs
         WHERE derived_source_id = ?1 ORDER BY parent_source_id LIMIT ?2",
    )?;
    let records = statement
        .query_map(rusqlite::params![source, remaining + 1], |row| {
            Ok(SourceRevisionRef {
                case_id: case_id.into(),
                source_id: row.get(0)?,
                revision: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if records.len() > remaining {
        return Err(limit_error());
    }
    Ok(records)
}

fn validate_topology(graph: &Graph, root: &str) -> DbResult<()> {
    let mut incoming = graph
        .keys()
        .map(|id| (id.as_str(), 0usize))
        .collect::<BTreeMap<_, _>>();
    for parents in graph.values() {
        for parent in parents {
            *incoming
                .get_mut(parent.as_str())
                .expect("validated dependency node") += 1;
        }
    }
    let mut pending = VecDeque::from([root]);
    let mut depths = BTreeMap::from([(root, 0usize)]);
    let mut visited = 0usize;
    while let Some(source) = pending.pop_front() {
        if incoming[source] != 0 {
            return Err(cycle_error());
        }
        visited += 1;
        let next_depth = depths[source] + 1;
        for parent in &graph[source] {
            if next_depth > MAX_DEPTH {
                return Err(limit_error());
            }
            let parent = parent.as_str();
            let depth = depths.entry(parent).or_default();
            *depth = (*depth).max(next_depth);
            let remaining = incoming.get_mut(parent).expect("validated dependency node");
            *remaining -= 1;
            if *remaining == 0 {
                pending.push_back(parent);
            }
        }
    }
    if visited != graph.len() {
        return Err(cycle_error());
    }
    Ok(())
}

fn cycle_error() -> DbError {
    DbError::System("structure view dependency cycle".into())
}

fn limit_error() -> DbError {
    DbError::System("structure dependency graph exceeds validation limits".into())
}
