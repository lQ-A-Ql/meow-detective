use domain::{structure::StructureNodeKind, CaseId, DataSource, EntryType, FileEntry};
use evidence_access::file_service::{read_file_bytes_for_case, SourceReadContext};
use persistence_source::repositories::{artifact_repo::ArtifactRepo, file_repo::FileRepo};
use persistence_sqlite::{
    repositories::{ceph_osd_repo::CephOsdRepo, source_revision_repo::SourceRevisionRepo},
    DbError,
};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::model::{Evidence, FactProjection};
use crate::structure_analysis::StructureAnalysisError;

const MAX_FILES: usize = 256;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;

pub(super) fn collect_source(
    conn: &rusqlite::Connection,
    root: &std::path::Path,
    case_id: &CaseId,
    source: &DataSource,
    state: &str,
    graph: &mut FactProjection,
) -> Result<(), StructureAnalysisError> {
    let revisions = SourceRevisionRepo::new(conn);
    let (reference, revision) = match revisions.current(&case_id.0, &source.id.0) {
        Ok(revision) => revision,
        Err(DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows)) => {
            graph.node(
                format!("source:{}@unknown", source.id.0),
                StructureNodeKind::SourceRevision,
                &source.name,
                json!({"sourceId": source.id.0, "status": "needs_evidence", "reason": "missing_revision"}),
            );
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    revisions.require_current(&reference)?;
    let ready =
        source_kernel::source_db::open_reconstruction_source_by_id(conn, root, case_id, &source.id)
            .map_err(|error| error.into_db_error())?;
    let evidence = Evidence {
        source_revision: reference.clone(),
        acquisition_id: revision.acquisition_id.0,
        file_id: None,
        artifact_id: None,
        parser: "structure.source".into(),
        parser_version: "1".into(),
        content_sha256: None,
        projection_sha256: None,
        locator: None,
        assertion_kind: "native_observation".into(),
        observed_at: None,
        request_started_at: None,
    };
    graph.node(
        format!("source:{}@{}", source.id.0, reference.revision),
        StructureNodeKind::SourceRevision,
        &source.name,
        json!({"sourceId": source.id.0, "revision": reference.revision, "acquisitionId": evidence.acquisition_id}),
    );
    super::backend::project(conn, root, case_id, source, state, graph)?;
    project_ceph(&ready.connection, source, &evidence, graph)?;
    if state == "ready" {
        let artifacts = ArtifactRepo::new(&ready.connection);
        let cursor_revision = artifacts.cursor_revision()?;
        artifacts.visit_by_family(Some("LinuxSystemConfig"), |artifact| {
            super::pve::project(&artifact, &evidence, graph)
                .map_err(|error| DbError::System(error.to_string()))
        })?;
        let mut journal = super::journal::JournalIdentities::default();
        artifacts.visit_by_family(Some("LinuxJournal"), |artifact| {
            journal.observe(&artifact, &evidence)
        })?;
        journal.project(&evidence, graph)?;
        if artifacts.cursor_revision()? != cursor_revision {
            return Err(StructureAnalysisError::Graph(
                "source artifacts changed during projection".into(),
            ));
        }
        project_files(
            conn,
            root,
            case_id,
            source,
            &ready.connection,
            &evidence,
            graph,
        )?;
    }
    revisions.require_current(&reference)?;
    Ok(())
}

fn project_ceph(
    conn: &rusqlite::Connection,
    source: &DataSource,
    evidence: &Evidence,
    graph: &mut FactProjection,
) -> Result<(), StructureAnalysisError> {
    for inventory in CephOsdRepo::new(conn).find_by_data_source(&source.id.0)? {
        let scope = inventory
            .ceph_fsid
            .clone()
            .unwrap_or_else(|| evidence.acquisition_id.clone());
        let cluster = graph.entity(&scope, "ceph", &scope, "Ceph");
        let osd = graph.entity(&scope, "osd", &inventory.osd_uuid, &inventory.description);
        let mut evidence = evidence.clone();
        evidence.parser = "ceph.bluestore_inventory".into();
        evidence.locator = Some(inventory.id.clone());
        evidence.projection_sha256 = Some(hex::encode(Sha256::digest(
            inventory.sanitized_metadata_json.as_bytes(),
        )));
        graph.fact(
            &osd,
            Some(&cluster),
            "member_of",
            &evidence,
            json!({
                "fsid": inventory.ceph_fsid,
                "osdUuid": inventory.osd_uuid,
                "selectedLabelEpoch": inventory.selected_epoch,
                "labelHealth": inventory.label_health,
                "dataSourceId": source.id.0,
            }),
        )?;
    }
    Ok(())
}

fn project_files(
    conn: &rusqlite::Connection,
    root: &std::path::Path,
    case_id: &CaseId,
    source: &DataSource,
    source_conn: &rusqlite::Connection,
    evidence: &Evidence,
    graph: &mut FactProjection,
) -> Result<(), StructureAnalysisError> {
    let mut files = Vec::new();
    FileRepo::new(source_conn).visit_all(Some(&source.id.0), |entry| {
        if entry.entry_type == EntryType::File && candidate(&entry.path) {
            if files.len() >= MAX_FILES {
                return Err(DbError::System(
                    "structure evidence file bound exceeded".into(),
                ));
            }
            files.push(entry);
        }
        Ok(())
    })?;
    files.sort_by(|left, right| left.path.cmp(&right.path).then(left.id.0.cmp(&right.id.0)));
    let mut context = SourceReadContext::new(source_conn, conn, root, case_id, &source.id);
    let mut total = 0;
    let mut host_files = Vec::new();
    for entry in files {
        if entry.size == Some(0) {
            continue;
        }
        let bytes = read_entry(&mut context, &entry, &mut total)?;
        let mut evidence = evidence.clone();
        evidence.file_id = Some(entry.id.0.clone());
        evidence.content_sha256 = Some(hex::encode(Sha256::digest(&bytes)));
        evidence.locator = Some(entry.path.clone());
        evidence.parser = "structure.json".into();
        let path = super::model::normalized_path(&entry.path);
        if super::host::is_host_path(&path) {
            host_files.push((entry, bytes, evidence));
        } else {
            super::kubernetes::project_file(&path, &bytes, &evidence, graph)?;
        }
    }
    super::host::project(host_files, evidence, graph)
}

fn candidate(path: &str) -> bool {
    let path = super::model::normalized_path(path);
    super::host::is_host_path(&path)
        || matches!(
            crate::cluster_service::kubernetes_paths::classify_kubernetes_path(&path),
            Some(crate::cluster_service::KubernetesArtifactKind::AuditLog)
        )
        || path.contains("/etc/kubernetes/") && path.ends_with(".json")
}

fn read_entry(
    context: &mut SourceReadContext<'_>,
    entry: &FileEntry,
    total: &mut u64,
) -> Result<Vec<u8>, StructureAnalysisError> {
    let size = entry
        .size
        .ok_or_else(|| StructureAnalysisError::Graph("evidence size is missing".into()))?;
    *total = total
        .checked_add(size)
        .ok_or_else(|| StructureAnalysisError::Graph("evidence size overflow".into()))?;
    if size > MAX_FILE_BYTES || *total > MAX_SOURCE_BYTES {
        return Err(StructureAnalysisError::Graph(
            "structure evidence byte bound exceeded".into(),
        ));
    }
    if entry.deleted || entry.encrypted {
        return Err(StructureAnalysisError::Graph(
            "structure evidence requires an accessible live file".into(),
        ));
    }
    let mut bytes = Vec::with_capacity(size as usize);
    while (bytes.len() as u64) < size {
        let length = (size - bytes.len() as u64).min(1024 * 1024) as u32;
        let chunk = read_file_bytes_for_case(&mut *context, &entry.id, bytes.len() as u64, length)
            .map_err(|error| StructureAnalysisError::Graph(error.to_string()))?;
        if chunk.len() != length as usize {
            return Err(StructureAnalysisError::Graph(
                "structure evidence read is incomplete".into(),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    if let Some(expected) = &entry.hash_sha256 {
        if hex::encode(Sha256::digest(&bytes)) != *expected {
            return Err(StructureAnalysisError::Graph(
                "structure evidence digest changed".into(),
            ));
        }
    }
    Ok(bytes)
}
