use std::collections::{BTreeMap, BTreeSet};

use petgraph::{
    acyclic::Acyclic,
    data::Build,
    stable_graph::{NodeIndex, StableDiGraph},
};

use super::{ActivityDefinition, RuntimeError};

#[derive(Clone)]
pub struct ActivityDag {
    limit: usize,
    edge_limit: usize,
    definitions: BTreeMap<String, ActivityDefinition>,
    graph: Acyclic<StableDiGraph<String, ()>>,
    indices: BTreeMap<String, NodeIndex>,
}

impl ActivityDag {
    pub fn new(limit: usize, edge_limit: usize) -> Result<Self, RuntimeError> {
        if limit == 0 || edge_limit == 0 {
            return Err(RuntimeError::Capacity);
        }
        Ok(Self {
            limit,
            edge_limit,
            definitions: BTreeMap::new(),
            graph: Acyclic::new(),
            indices: BTreeMap::new(),
        })
    }

    pub fn insert_batch(
        &mut self,
        activities: Vec<ActivityDefinition>,
    ) -> Result<BTreeSet<String>, RuntimeError> {
        let mut additions = BTreeMap::new();
        for activity in activities {
            let key = activity.key()?;
            if let Some(existing) = self.definitions.get(&key).or_else(|| additions.get(&key)) {
                if existing != &activity {
                    return Err(RuntimeError::ConflictingWork);
                }
            } else {
                if self.definitions.len() + additions.len() >= self.limit {
                    return Err(RuntimeError::Capacity);
                }
                additions.insert(key, activity);
            }
        }
        if additions.is_empty() {
            return Ok(BTreeSet::new());
        }
        let mut edge_count = self.edge_count();
        for definition in additions.values() {
            edge_count = edge_count
                .checked_add(definition.dependencies.len())
                .filter(|count| *count <= self.edge_limit)
                .ok_or(RuntimeError::Capacity)?;
            if definition.dependencies.iter().any(|dependency| {
                !self.indices.contains_key(dependency) && !additions.contains_key(dependency)
            }) {
                return Err(RuntimeError::MissingDependency);
            }
        }
        let mut graph = self.graph.clone();
        let mut indices = self.indices.clone();
        for key in additions.keys() {
            indices.insert(key.clone(), graph.add_node(key.clone()));
        }
        for (key, definition) in &additions {
            for dependency in &definition.dependencies {
                graph
                    .try_add_edge(indices[dependency], indices[key], ())
                    .map_err(|_| RuntimeError::DependencyCycle)?;
            }
        }
        let inserted = additions.keys().cloned().collect();
        self.graph = graph;
        self.indices = indices;
        self.definitions.extend(additions);
        Ok(inserted)
    }

    pub fn get(&self, key: &str) -> Option<&ActivityDefinition> {
        self.definitions.get(key)
    }

    pub fn definitions(&self) -> impl Iterator<Item = (&String, &ActivityDefinition)> {
        self.definitions.iter()
    }

    pub fn node_count(&self) -> usize {
        self.graph.inner().node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.inner().edge_count()
    }
}
