use domain::{Artifact, ArtifactId, CaseId, DataSourceId, GraphNode, NodeType};
use evidence_core::carving::CarveCandidate;
use persistence_source::repositories::artifact_repo::ArtifactRepo;
use persistence_sqlite::repositories::{
    fingerprint_repo::ForensicFingerprintRepo,
    graph_repo::GraphRepo,
    source_meta_repo::{SourceMetaRepo, ARTIFACT_CURSOR_REVISION_KEY},
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use transport::{dto::CarvedArtifactDto, CommandError};

pub(super) fn persist(
    conn: &Connection,
    case_root: &std::path::Path,
    case: &CaseId,
    source: &DataSourceId,
    candidates: &[CarveCandidate],
    cancel: &AtomicBool,
) -> Result<Vec<CarvedArtifactDto>, CommandError> {
    let repo = ArtifactRepo::new(conn);
    let mut output = Vec::with_capacity(candidates.len());
    let mut nodes = Vec::new();
    let mut artifacts = Vec::new();
    for candidate in candidates {
        let artifact = artifact(candidate)?;
        if let Some(previous) = repo
            .find_by_id(&artifact.id.0)
            .map_err(CommandError::from_service_error)?
        {
            output.push(super::access::dto(&previous, source)?);
        } else {
            nodes.push(GraphNode {
                id: artifact.id.0.clone(),
                case_id: case.0.clone(),
                node_type: NodeType::Artifact,
                label: artifact.title.clone(),
                summary: artifact.summary.clone(),
                tags: vec!["carving".into(), "png".into()],
                created_at: artifact.created_at.to_rfc3339(),
            });
            output.push(super::access::dto(&artifact, source)?);
            artifacts.push(artifact);
        }
    }
    if artifacts.is_empty() {
        return Ok(output);
    }
    let mut staged =
        crate::analysis_service::stage_native_records(conn, case_root, &source.0, &artifacts, &[])
            .map_err(|error| CommandError::internal(error.to_string()))?;
    let transaction = conn
        .unchecked_transaction()
        .map_err(CommandError::from_service_error)?;
    persistence_source::detail::bind_generation(&transaction, &staged.name)
        .map_err(CommandError::from_service_error)?;
    persist_artifact_fingerprints(&transaction, case, source, &artifacts)?;
    GraphRepo::new(&transaction)
        .insert_nodes_in_transaction(&nodes)
        .map_err(CommandError::from_service_error)?;
    if !nodes.is_empty() {
        SourceMetaRepo::new(&transaction)
            .bump_revision(ARTIFACT_CURSOR_REVISION_KEY)
            .map_err(CommandError::from_service_error)?;
    }
    if cancel.load(Ordering::Acquire) {
        return Err(CommandError::cancelled("Carving cancelled"));
    }
    transaction
        .commit()
        .map_err(CommandError::from_service_error)?;
    staged.commit();
    Ok(output)
}

fn persist_artifact_fingerprints(
    conn: &rusqlite::Transaction<'_>,
    case: &CaseId,
    source: &DataSourceId,
    artifacts: &[Artifact],
) -> Result<(), CommandError> {
    let repository = ForensicFingerprintRepo::new(conn);
    if !repository
        .is_available()
        .map_err(CommandError::from_service_error)?
    {
        return Ok(());
    }
    for artifact in artifacts {
        repository
            .upsert_in_transaction(&domain::ForensicFingerprint::for_artifact(
                artifact, &case.0, &source.0,
            ))
            .map_err(CommandError::from_service_error)?;
    }
    Ok(())
}
fn artifact(candidate: &CarveCandidate) -> Result<Artifact, CommandError> {
    let length = candidate
        .length
        .ok_or_else(|| CommandError::internal("Unverified carving length"))?;
    let digest = candidate
        .sha256
        .as_ref()
        .ok_or_else(|| CommandError::internal("Unverified carving digest"))?;
    let identity = serde_json::json!([
        candidate.source_id,
        candidate.revision,
        candidate.view,
        candidate.offset,
        length,
        candidate.validator_version,
        digest
    ])
    .to_string();
    let is_zip = candidate.validator_version == evidence_core::carving::ZIP_VALIDATOR_VERSION;
    let family = if is_zip { "carving.zip" } else { "carving.png" };
    let format_name = if is_zip { "ZIP" } else { "PNG" };
    let id = format!(
        "carved-{}:{}",
        format_name.to_ascii_lowercase(),
        hex::encode(Sha256::digest(identity))
    );
    let attrs = serde_json::json!({"dataSourceId":candidate.source_id,"sourceRevision":candidate.revision,"view":candidate.view,
        "addressSpace":"logicalDisk","offset":candidate.offset.to_string(),"length":length.to_string(),"sha256":digest,
        "hashScope":"carvedBytes","validatorVersion":candidate.validator_version,"validation":candidate.reason,"originalFileIdentity":"unknown"});
    Ok(Artifact {
        id: ArtifactId(id),
        family: family.into(),
        title: format!("{format_name} candidate @ {}", candidate.offset),
        summary: format!(
            "{} bytes; structure verified; original file identity unknown",
            length
        ),
        source_object_id: None,
        extractor_id: Some(family.into()),
        extractor_version: Some(candidate.validator_version.into()),
        confidence: Some(0.9),
        source_attribution: Some(candidate.source_id.clone()),
        created_at: chrono::Utc::now(),
        attrs: attrs
            .as_object()
            .ok_or_else(|| CommandError::internal("Carving metadata failed"))?
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    })
}
