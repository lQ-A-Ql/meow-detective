use std::collections::{BTreeMap, BTreeSet};

use domain::structure::{
    SourceRevisionRef, StructureEdge, StructureGraph, StructureNode, StructureNodeId,
    StructureNodeKind, StructureRelationKind, MAX_GRAPH_EDGES, MAX_GRAPH_NODES,
};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::structure_analysis::StructureAnalysisError;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Evidence {
    pub source_revision: SourceRevisionRef,
    pub acquisition_id: String,
    pub file_id: Option<String>,
    pub artifact_id: Option<String>,
    pub parser: String,
    pub parser_version: String,
    pub content_sha256: Option<String>,
    pub projection_sha256: Option<String>,
    pub locator: Option<String>,
    pub assertion_kind: String,
    pub observed_at: Option<String>,
    pub request_started_at: Option<String>,
}

#[derive(Default)]
pub(super) struct FactProjection {
    pub nodes: BTreeMap<StructureNodeId, StructureNode>,
    pub edges: BTreeMap<
        (
            StructureNodeId,
            StructureNodeId,
            StructureRelationKind,
            String,
        ),
        StructureEdge,
    >,
    placements: BTreeMap<(String, String), Vec<(String, StructureNodeId)>>,
    host_uuids: BTreeMap<String, BTreeSet<StructureNodeId>>,
    pub backends: Vec<super::backend::Backend>,
    pub backend_requests: Vec<super::backend::BackendRequest>,
    pub claims: BTreeMap<String, Vec<super::backend::ObservedClaim>>,
}

impl FactProjection {
    pub fn node(
        &mut self,
        id: String,
        kind: StructureNodeKind,
        label: &str,
        details: Value,
    ) -> String {
        self.nodes
            .entry(StructureNodeId(id.clone()))
            .or_insert_with(|| StructureNode {
                id: StructureNodeId(id.clone()),
                kind,
                label: label.into(),
                details,
            });
        id
    }

    pub fn entity(&mut self, scope: &str, kind: &str, native: &str, label: &str) -> String {
        let node_kind = match kind {
            "physical_host" => StructureNodeKind::PhysicalHost,
            "namespace" => StructureNodeKind::Namespace,
            "pvc" | "pv" | "disk_reference" => StructureNodeKind::StorageObject,
            "pve" | "kubernetes" | "ceph" => StructureNodeKind::Environment,
            _ => StructureNodeKind::Workload,
        };
        self.node(
            key("entity", &[scope, kind, native]),
            node_kind,
            label,
            json!({
                "entityKind": kind,
                "identityState": "candidate",
                "status": "needs_evidence",
                "contentCapability": "metadata_only",
                "identityScope": scope,
                "nativeIdentity": native,
            }),
        )
    }

    pub fn fact(
        &mut self,
        subject: &str,
        target: Option<&str>,
        relation: &str,
        evidence: &Evidence,
        attributes: Value,
    ) -> Result<String, StructureAnalysisError> {
        let details = json!({
            "subjectId": subject,
            "targetId": target,
            "relationKind": relation,
            "evidence": evidence,
            "attributes": attributes,
            "status": if evidence.observed_at.is_some() { "observed" } else { "candidate" },
            "conflictIds": [],
        });
        let id = key("fact", &[&serde_json::to_string(&details)?]);
        self.node(
            id.clone(),
            StructureNodeKind::EvidenceFact,
            relation,
            details,
        );
        self.edge(&id, subject, StructureRelationKind::Observation, "asserts");
        self.edge(
            &id,
            &format!(
                "source:{}@{}",
                evidence.source_revision.source_id, evidence.source_revision.revision
            ),
            StructureRelationKind::Observation,
            "evidence_from",
        );
        if let Some(target) = target {
            self.edge(
                &id,
                target,
                StructureRelationKind::DeploymentControl,
                relation,
            );
        }
        self.check_bounds()?;
        Ok(id)
    }

    pub fn edge(&mut self, from: &str, to: &str, kind: StructureRelationKind, role: &str) {
        let key = (
            StructureNodeId(from.into()),
            StructureNodeId(to.into()),
            kind,
            role.into(),
        );
        self.edges.entry(key.clone()).or_insert(StructureEdge {
            from: key.0,
            to: key.1,
            kind,
            role: role.into(),
            required: kind == StructureRelationKind::ContentDependency,
        });
    }

    pub fn placement(&mut self, subject: &str, target: &str, fact: &str, evidence: &Evidence) {
        if evidence.observed_at.is_none() {
            return;
        }
        self.placements
            .entry((subject.into(), "pod_node_assignment".into()))
            .or_default()
            .push((target.into(), StructureNodeId(fact.into())));
    }

    pub fn host_uuid(&mut self, uuid: &str, host: &str) {
        self.host_uuids
            .entry(uuid.into())
            .or_default()
            .insert(StructureNodeId(host.into()));
    }

    pub fn finish(mut self) -> Result<StructureGraph, StructureAnalysisError> {
        super::backend::resolve(&mut self)?;
        for placements in self.placements.values() {
            let targets = placements
                .iter()
                .map(|(target, _)| target)
                .collect::<BTreeSet<_>>();
            if targets.len() > 1 {
                let conflicts = placements.iter().map(|(_, id)| &id.0).collect::<Vec<_>>();
                for (_, id) in placements {
                    if let Some(node) = self.nodes.get_mut(id) {
                        node.details["status"] = json!("conflicted");
                        node.details["conflictIds"] = json!(conflicts);
                    }
                }
            }
        }
        for hosts in self.host_uuids.values().filter(|hosts| hosts.len() > 1) {
            for host in hosts {
                if let Some(node) = self.nodes.get_mut(host) {
                    node.details["identityState"] = json!("conflicted");
                    node.details["status"] = json!("conflicted");
                }
            }
        }
        self.check_bounds()?;
        let graph = StructureGraph {
            nodes: self.nodes.into_values().collect(),
            edges: self.edges.into_values().collect(),
            mappings: Vec::new(),
            recovery_relations: Vec::new(),
        };
        graph
            .validate()
            .map_err(|error| StructureAnalysisError::Graph(error.to_string()))?;
        Ok(graph)
    }

    fn check_bounds(&self) -> Result<(), StructureAnalysisError> {
        if self.nodes.len() > MAX_GRAPH_NODES || self.edges.len() > MAX_GRAPH_EDGES {
            return Err(StructureAnalysisError::Graph(
                "structure fact bound exceeded".into(),
            ));
        }
        Ok(())
    }
}

pub(super) fn key(prefix: &str, values: &[&str]) -> String {
    let mut digest = Sha256::new();
    digest.update(prefix.as_bytes());
    for value in values {
        digest.update((value.len() as u64).to_le_bytes());
        digest.update(value.as_bytes());
    }
    format!("{prefix}:{}", hex::encode(digest.finalize()))
}

pub(super) fn normalized_time(
    value: Option<&str>,
) -> Result<Option<String>, StructureAnalysisError> {
    value
        .map(chrono::DateTime::parse_from_rfc3339)
        .transpose()
        .map(|value| {
            value.map(|value| {
                value
                    .with_timezone(&chrono::Utc)
                    .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
            })
        })
        .map_err(|error| {
            StructureAnalysisError::Graph(format!("invalid evidence timestamp: {error}"))
        })
}

pub(super) fn normalized_path(path: &str) -> String {
    format!("/{}", path.replace('\\', "/").trim_start_matches('/'))
}
