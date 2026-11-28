use std::collections::{BTreeMap, BTreeSet};

use petgraph::{acyclic::Acyclic, stable_graph::StableDiGraph};

use super::{ActivityDefinition, RuntimeError};

#[derive(Clone)]
pub struct ActivityDag {
    limit: usize,
    edge_limit: usize,
    definitions: BTreeMap<String, ActivityDefinition>,
    graph: Acyclic<StableDiGraph<String, ()>>,
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
        })
    }

    pub fn insert_batch(
        &mut self,
        activities: Vec<ActivityDefinition>,
    ) -> Result<BTreeSet<String>, RuntimeError> {
        let mut definitions = self.definitions.clone();
        let mut inserted = BTreeSet::new();
        for activity in activities {
            let key = activity.key()?;
            if let Some(existing) = definitions.get(&key) {
                if existing != &activity {
                    return Err(RuntimeError::ConflictingWork);
                }
            } else {
                if definitions.len() >= self.limit {
                    return Err(RuntimeError::Capacity);
                }
                inserted.insert(key.clone());
                definitions.insert(key, activity);
            }
        }
        let graph = build_graph(&definitions, self.edge_limit)?;
        self.definitions = definitions;
        self.graph = graph;
        Ok(inserted)
    }

    pub fn get(&self, key: &str) -> Option<&ActivityDefinition> {
        self.definitions.get(key)
    }

    pub fn definitions(&self) -> impl Iterator<Item = (&String, &ActivityDefinition)> {
        self.definitions.iter()
    }
}

fn build_graph(
    definitions: &BTreeMap<String, ActivityDefinition>,
    edge_limit: usize,
) -> Result<Acyclic<StableDiGraph<String, ()>>, RuntimeError> {
    let mut graph = StableDiGraph::new();
    let indices = definitions
        .keys()
        .map(|key| (key.as_str(), graph.add_node(key.clone())))
        .collect::<BTreeMap<_, _>>();
    let mut graph = Acyclic::try_from_graph(graph).map_err(|_| RuntimeError::DependencyCycle)?;
    let mut edges = 0usize;
    for (key, definition) in definitions {
        for dependency in &definition.dependencies {
            edges += 1;
            if edges > edge_limit {
                return Err(RuntimeError::Capacity);
            }
            let parent = *indices
                .get(dependency.as_str())
                .ok_or(RuntimeError::MissingDependency)?;
            graph
                .try_add_edge(parent, indices[key.as_str()], ())
                .map_err(|_| RuntimeError::DependencyCycle)?;
        }
    }
    Ok(graph)
}
