use domain::{DataSourceId, EntryType, FileEntryId, KubernetesScopeId};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, file_repo::FileRepo,
    linux_topology_scope_repo::LinuxTopologyScopeRepo,
};
use rusqlite::Connection;

use crate::source_db;

use super::kubernetes_paths::{
    classify_kubernetes_path, KubernetesArtifactKind, MAX_KUBERNETES_ARTIFACTS,
};
use super::{ClusterServiceError, Result, TopologyScopeKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubernetesMemberArtifact {
    pub data_source_id: DataSourceId,
    pub file_id: FileEntryId,
    pub path: String,
    pub kind: KubernetesArtifactKind,
    pub size: u64,
    pub deleted: bool,
    pub encrypted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubernetesMemberArtifactInventory {
    pub data_source_id: DataSourceId,
    pub scanned_entries: u64,
    pub artifacts: Vec<KubernetesMemberArtifact>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubernetesClusterArtifactInventory {
    pub scope_id: String,
    pub expected_member_count: u32,
    pub members: Vec<KubernetesMemberArtifactInventory>,
}

pub fn discover_kubernetes_member_artifacts(
    connection: &Connection,
    data_source_id: &DataSourceId,
) -> Result<KubernetesMemberArtifactInventory> {
    let entries = FileRepo::new(connection).find_by_data_source(data_source_id)?;
    let scanned_entries = entries.len() as u64;
    let mut artifacts = entries
        .into_iter()
        .filter(|entry| entry.entry_type == EntryType::File)
        .filter_map(|entry| {
            let kind = classify_kubernetes_path(&entry.path)?;
            Some(KubernetesMemberArtifact {
                data_source_id: entry.data_source_id,
                file_id: entry.id,
                path: entry.path,
                kind,
                size: entry.size.unwrap_or_default(),
                deleted: entry.deleted,
                encrypted: entry.encrypted,
            })
        })
        .collect::<Vec<_>>();

    artifacts.sort_by(|left, right| {
        normalize_path(&left.path)
            .cmp(&normalize_path(&right.path))
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.file_id.0.cmp(&right.file_id.0))
    });
    let truncated = artifacts.len() > MAX_KUBERNETES_ARTIFACTS;
    artifacts.truncate(MAX_KUBERNETES_ARTIFACTS);

    Ok(KubernetesMemberArtifactInventory {
        data_source_id: data_source_id.clone(),
        scanned_entries,
        artifacts,
        truncated,
    })
}

pub fn discover_kubernetes_cluster_artifacts(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &domain::CaseId,
    scope_id: &KubernetesScopeId,
) -> Result<KubernetesClusterArtifactInventory> {
    let scope_repo = LinuxTopologyScopeRepo::new(case_connection);
    let scope_kind = scope_repo.find_kind(&scope_id.0)?;
    if scope_kind.as_deref() != Some(TopologyScopeKind::Kubernetes.as_str()) {
        return Err(ClusterServiceError::InvalidClusterId);
    }
    let source_ids =
        DataSourceRepo::new(case_connection).find_ids_by_topology_scope(case_id, &scope_id.0)?;
    let expected_member_count = source_ids.len() as u32;
    let mut members = Vec::with_capacity(source_ids.len());
    for source_id in source_ids {
        let ready = source_db::open_ready_source_read_only_by_id(
            case_connection,
            case_root,
            case_id,
            &source_id,
        )
        .map_err(|error| ClusterServiceError::Db(error.into_db_error()))?;
        members.push(discover_kubernetes_member_artifacts(
            &ready.connection,
            &ready.data_source_id,
        )?);
    }

    Ok(KubernetesClusterArtifactInventory {
        scope_id: scope_id.0.clone(),
        expected_member_count,
        members,
    })
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}
