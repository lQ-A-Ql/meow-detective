use domain::{
    structure::{StorageObjectVersionKind, StructureNodeKind, StructureRelationKind},
    CaseId, DataSource, DataSourceKind,
};
use persistence_sqlite::repositories::{
    catalog_publication_repo::CatalogPublicationRepo,
    ceph_fs_lineage_repo::CephFsDerivedLineageRepo, source_revision_repo::SourceRevisionRepo,
    storage_object_repo::StorageObjectRepo,
};
use serde_json::{json, Value};

use super::model::{Evidence, FactProjection};
use crate::structure_analysis::StructureAnalysisError;

pub(super) struct Backend {
    pub cluster: String,
    pub filesystem: String,
    pub version_node: String,
    pub capability: String,
    pub receipt: String,
}

pub(super) struct BackendRequest {
    pub subject: String,
    pub claim: Option<String>,
    pub cluster: String,
    pub filesystem: String,
    pub root: String,
    pub evidence: Evidence,
    pub volume_name: String,
    pub claim_namespace: String,
}

pub(super) struct ObservedClaim {
    pub volume_name: String,
    pub namespace: String,
    pub evidence: Evidence,
}

pub(super) fn project(
    conn: &rusqlite::Connection,
    root: &std::path::Path,
    case_id: &CaseId,
    source: &DataSource,
    state: &str,
    graph: &mut FactProjection,
) -> Result<(), StructureAnalysisError> {
    if !matches!(
        source.kind,
        DataSourceKind::CephFs | DataSourceKind::CephRbd
    ) {
        return Ok(());
    }
    let revisions = SourceRevisionRepo::new(conn);
    let versions = StorageObjectRepo::new(conn).versions_for_source(&case_id.0, &source.id.0)?;
    let mut published = None;
    if state == "ready" {
        if source.kind == DataSourceKind::CephFs {
            ceph_reconstruction::ceph_reconstruction::verify_cephfs_published_source(
                conn, root, source,
            )
            .map_err(|error| StructureAnalysisError::Graph(error.to_string()))?;
        }
        persistence_source::publication::catalog_publication::verify_registered_catalog(
            conn, root, case_id, &source.id,
        )
        .map_err(|error| StructureAnalysisError::Graph(error.to_string()))?;
        published = CatalogPublicationRepo::new(conn).find(&source.id)?;
    }
    for version in versions
        .iter()
        .filter(|version| version.version_kind == StorageObjectVersionKind::DerivedOutput)
    {
        for parent in &version.parent_source_revisions {
            revisions.require_current(parent)?;
        }
        let id = format!("version:{}", version.id.0);
        graph.node(id.clone(), StructureNodeKind::ObjectVersion, &source.name, json!({
            "storageObjectId": version.storage_object_id,
            "sourceId": source.id.0,
            "nativeObjectId": version.native_object_id,
            "nativeVersions": version.native_versions,
            "acquisitionIds": version.acquisition_ids,
            "parentRevisions": version.parent_source_revisions,
            "mappingSha256": version.mapping_sha256,
            "status": if published.is_some() { "published" } else { "needs_evidence" },
            "publicationReceipt": published.as_ref().map(|publication| &publication.catalog_digest),
            "contentCapability": "metadata_only",
        }));
        let Some(publication) = &published else {
            continue;
        };
        let object = StorageObjectRepo::new(conn)
            .find(&version.storage_object_id)?
            .ok_or_else(|| {
                StructureAnalysisError::Graph("Ceph storage object is missing".into())
            })?;
        if object.identity_state != "proven" {
            continue;
        }
        if version.consistency != domain::structure::RevisionConsistency::Coherent {
            continue;
        }
        if source.kind == DataSourceKind::CephFs {
            let lineage = CephFsDerivedLineageRepo::new(conn)
                .find_by_data_source(&source.id.0)?
                .ok_or_else(|| StructureAnalysisError::Graph("CephFS lineage is missing".into()))?;
            if lineage.lineage.parent_storage_object_id != version.storage_object_id
                || lineage.lineage.lineage_fingerprint != version.mapping_sha256
            {
                return Err(StructureAnalysisError::Graph(
                    "CephFS public object binding differs from lineage".into(),
                ));
            }
            graph
                .nodes
                .get_mut(&domain::structure::StructureNodeId(id.clone()))
                .expect("registered version node")
                .details["contentCapability"] = json!(lineage.lineage.source_capability);
            graph.backends.push(Backend {
                cluster: lineage.lineage.cluster_identity,
                filesystem: lineage.lineage.filesystem_name,
                version_node: id,
                capability: lineage.lineage.source_capability,
                receipt: publication.catalog_digest.clone(),
            });
        }
    }
    Ok(())
}

pub(super) fn resolve(graph: &mut FactProjection) -> Result<(), StructureAnalysisError> {
    let requests = std::mem::take(&mut graph.backend_requests);
    for request in requests {
        let matching = graph
            .backends
            .iter()
            .filter(|backend| {
                uuid::Uuid::parse_str(&request.cluster)
                    .ok()
                    .filter(|uuid| !uuid.is_nil())
                    .zip(uuid::Uuid::parse_str(&backend.cluster).ok())
                    .is_some_and(|(requested, native)| requested == native)
                    && backend.filesystem == request.filesystem
            })
            .collect::<Vec<_>>();
        let (version, capability, receipt) = match matching.as_slice() {
            [backend]
                if request.root == "/"
                    && backend.capability == "bounded-preview"
                    && request.evidence.assertion_kind == "observed_response"
                    && request.evidence.observed_at.is_some() =>
            {
                (
                    Some(backend.version_node.clone()),
                    "bounded_preview",
                    Some(backend.receipt.clone()),
                )
            }
            _ => (None, "metadata_only", None),
        };
        let attributes = json!({
            "backendState": if version.is_some() { "version_bound" } else { "needs_evidence" },
            "contentCapability": capability,
            "backendVersion": version,
            "publicationReceipt": receipt,
            "clusterIdentity": request.cluster,
            "filesystem": request.filesystem,
            "rootPath": request.root,
            "bindingKind": "available_published_view",
            "historicalContentAtObservation": "unproven",
        });
        let fact = graph.fact(
            &request.subject,
            version.as_deref(),
            "backend_binding",
            &request.evidence,
            attributes,
        )?;
        if let Some(version) = &version {
            graph.edge(
                &request.subject,
                version,
                StructureRelationKind::ContentDependency,
                "reads_published_version",
            );
            graph.edge(
                &fact,
                version,
                StructureRelationKind::ContentDependency,
                "reads_published_version",
            );
            if let Some(claim) = &request.claim {
                let Some(claim_evidence) = matching_claim(graph, &request, claim) else {
                    continue;
                };
                let claim_fact = graph.fact(
                    claim,
                    Some(version),
                    "claim_backend_binding",
                    &request.evidence,
                    json!({
                        "pvId": request.subject,
                        "contentCapability": capability,
                        "publicationReceipt": receipt,
                        "claimEvidence": claim_evidence,
                        "bindingKind": "available_published_view",
                        "historicalContentAtObservation": "unproven",
                    }),
                )?;
                graph.edge(
                    &claim_fact,
                    version,
                    StructureRelationKind::ContentDependency,
                    "reads_published_version",
                );
                graph.edge(
                    claim,
                    version,
                    StructureRelationKind::ContentDependency,
                    "reads_published_version",
                );
            }
        }
    }
    Ok(())
}

pub(super) fn csi_request(
    graph: &mut FactProjection,
    subject: &str,
    claim: Option<String>,
    spec: &Value,
    volume_name: &str,
    evidence: &Evidence,
) {
    let Some(csi) = spec.get("csi") else { return };
    let driver = csi
        .get("driver")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if driver != "cephfs.csi.ceph.com" && !driver.ends_with(".cephfs.csi.ceph.com") {
        return;
    }
    let attributes = csi.get("volumeAttributes").unwrap_or(&Value::Null);
    let text = |key: &str| {
        attributes
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    if text("staticVolume") != "true" || text("rootPath") != "/" {
        return;
    }
    graph.backend_requests.push(BackendRequest {
        subject: subject.into(),
        claim,
        cluster: text("clusterID"),
        filesystem: text("fsName"),
        root: text("rootPath"),
        evidence: evidence.clone(),
        volume_name: volume_name.into(),
        claim_namespace: spec
            .pointer("/claimRef/namespace")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
    });
}

fn matching_claim(
    graph: &FactProjection,
    request: &BackendRequest,
    claim: &str,
) -> Option<Evidence> {
    let time = request.evidence.observed_at.as_ref()?;
    let claims = graph.claims.get(claim)?;
    let latest = claims
        .iter()
        .filter_map(|claim| claim.evidence.observed_at.as_ref())
        .filter(|observed| *observed <= time)
        .max()?;
    let candidates = claims
        .iter()
        .filter(|claim| claim.evidence.observed_at.as_ref() == Some(latest))
        .collect::<Vec<_>>();
    if candidates.iter().any(|claim| {
        claim.volume_name != request.volume_name || claim.namespace != request.claim_namespace
    }) {
        return None;
    }
    candidates.first().map(|claim| claim.evidence.clone())
}
