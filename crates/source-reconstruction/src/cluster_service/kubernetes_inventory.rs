use domain::{DataSourceId, EntryType, FileEntryId, KubernetesScopeId};
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use rusqlite::Connection;
use std::collections::BTreeMap;

use source_kernel::source_db;

use super::kubernetes_paths::{
    classify_kubernetes_path, KubernetesArtifactKind, MAX_KUBERNETES_ARTIFACTS,
};
use super::{ClusterServiceError, Result};

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
    pub matched_entries: usize,
    pub control_plane_observed: bool,
    pub artifacts: Vec<KubernetesMemberArtifact>,
    pub truncated: bool,
}

impl KubernetesMemberArtifactInventory {
    pub fn require_complete(&self) -> std::result::Result<(), super::KubernetesParserError> {
        if self.truncated {
            return Err(super::KubernetesParserError::Limit {
                kind: "artifact inventory entries",
                actual: self.matched_entries,
                max: MAX_KUBERNETES_ARTIFACTS,
            });
        }
        Ok(())
    }
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
    let mut collector = MemberArtifactCollector::new(data_source_id.clone());
    FileRepo::new(connection).visit_all(Some(&data_source_id.0), |entry| {
        collector.observe(entry);
        Ok(())
    })?;
    Ok(collector.finish())
}

pub(super) struct MemberArtifactCollector {
    source: DataSourceId,
    scanned_entries: u64,
    matched_entries: usize,
    control_plane_observed: bool,
    truncated: bool,
    artifacts: BTreeMap<(String, KubernetesArtifactKind, String), KubernetesMemberArtifact>,
}

impl MemberArtifactCollector {
    pub(super) fn new(source: DataSourceId) -> Self {
        Self {
            source,
            scanned_entries: 0,
            matched_entries: 0,
            control_plane_observed: false,
            truncated: false,
            artifacts: BTreeMap::new(),
        }
    }

    pub(super) fn observe(&mut self, entry: domain::FileEntry) {
        self.scanned_entries += 1;
        if entry.entry_type != EntryType::File {
            return;
        }
        let Some(kind) = classify_kubernetes_path(&entry.path) else {
            return;
        };
        self.matched_entries += 1;
        self.control_plane_observed |= matches!(
            kind,
            KubernetesArtifactKind::StaticPodManifest
                | KubernetesArtifactKind::EtcdBackend
                | KubernetesArtifactKind::EtcdWal
        );
        let key = (normalize_path(&entry.path), kind, entry.id.0.clone());
        self.artifacts.insert(
            key,
            KubernetesMemberArtifact {
                data_source_id: entry.data_source_id,
                file_id: entry.id,
                path: entry.path,
                kind,
                size: entry.size.unwrap_or_default(),
                deleted: entry.deleted,
                encrypted: entry.encrypted,
            },
        );
        if self.artifacts.len() > MAX_KUBERNETES_ARTIFACTS {
            self.artifacts.pop_last();
            self.truncated = true;
        }
    }

    pub(super) fn finish(self) -> KubernetesMemberArtifactInventory {
        KubernetesMemberArtifactInventory {
            data_source_id: self.source,
            scanned_entries: self.scanned_entries,
            matched_entries: self.matched_entries,
            control_plane_observed: self.control_plane_observed,
            artifacts: self.artifacts.into_values().collect(),
            truncated: self.truncated,
        }
    }
}

pub fn discover_kubernetes_cluster_artifacts(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &domain::CaseId,
    scope_id: &KubernetesScopeId,
) -> Result<KubernetesClusterArtifactInventory> {
    let scope = super::require_kubernetes_scope(case_connection, &case_id.0, scope_id)?;
    let source_ids =
        DataSourceRepo::new(case_connection).find_ids_by_topology_scope(case_id, &scope_id.0)?;
    let expected_member_count = scope.member_count;
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
