use std::collections::BTreeMap;

use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::{
    ceph_osd_repo::CephOsdRepo,
    datasource_repo::DataSourceRepo,
    file_repo::FileRepo,
    linux_import_set_repo::LinuxImportSetRepo,
    linux_topology_artifact_repo::{LinuxTopologyArtifactRecord, LinuxTopologyArtifactRepo},
    linux_topology_edge_repo::LinuxTopologyEdgeRepo,
    linux_topology_membership_repo::LinuxTopologyMembershipRepo,
    linux_topology_scope_repo::LinuxTopologyScopeRepo,
    storage_object_repo::{StorageObjectRecord, StorageObjectRepo},
};

use crate::source_db;

use super::{
    discover_kubernetes_member_artifacts, ClusterServiceError, Result, TopologyEdgeKind,
    TopologyMemberRole, TopologyScopeKind,
};

mod pve;
use pve::register_pve_scope;
mod environment;
use environment::project_environment_objects;
mod helpers;
use helpers::is_pve_path;
use helpers::{
    edge_record, insert_memberships, membership_record, pve_qemu_vm_id, replace_scope, scope_record,
};

struct SourceObservations {
    pve_sources: Vec<(u32, String)>,
    pve_virtual_machines: BTreeMap<String, Vec<(u32, String, String)>>,
    ceph_sources: Vec<(u32, String)>,
    kubernetes_sources: Vec<(u32, String, TopologyMemberRole)>,
    kubernetes_artifacts: Vec<super::kubernetes_inventory::KubernetesMemberArtifact>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportSetTopologyProjection {
    pub os_scope_ids: Vec<String>,
    pub pve_scope_id: Option<String>,
    pub ceph_scope_id: Option<String>,
    pub virtual_machine_scope_ids: Vec<String>,
    pub kubernetes_scope_id: Option<String>,
}

pub fn project_import_set_topology(
    case_connection: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    import_set_id: &str,
) -> Result<ImportSetTopologyProjection> {
    let members = LinuxImportSetRepo::new(case_connection).find_members(import_set_id)?;
    if members.is_empty() {
        return Err(ClusterServiceError::InvalidClusterId);
    }
    if members.iter().any(|member| {
        !matches!(member.import_state.as_str(), "ready" | "ready_metadata")
            || member.data_source_id.as_deref().is_none_or(str::is_empty)
    }) {
        return Err(ClusterServiceError::IncompleteImportSet);
    }
    let source_members = members
        .into_iter()
        .filter_map(|member| {
            member
                .data_source_id
                .map(|source_id| (member.member_index, source_id))
        })
        .collect::<Vec<_>>();
    if source_members.is_empty() {
        return Err(ClusterServiceError::InsufficientSources);
    }

    let mut projection = ImportSetTopologyProjection::default();
    let scope_repo = LinuxTopologyScopeRepo::new(case_connection);
    let membership_repo = LinuxTopologyMembershipRepo::new(case_connection);
    let edge_repo = LinuxTopologyEdgeRepo::new(case_connection);
    let artifact_repo = LinuxTopologyArtifactRepo::new(case_connection);
    let storage_repo = StorageObjectRepo::new(case_connection);
    let SourceObservations {
        pve_sources,
        pve_virtual_machines,
        ceph_sources,
        kubernetes_sources,
        kubernetes_artifacts,
    } = collect_source_observations(
        case_connection,
        case_root,
        case_id,
        import_set_id,
        &source_members,
        &scope_repo,
        &membership_repo,
        &edge_repo,
        &mut projection,
    )?;
    register_detected_scopes(
        case_id,
        import_set_id,
        &scope_repo,
        &membership_repo,
        &edge_repo,
        &artifact_repo,
        &storage_repo,
        &mut projection,
        pve_sources,
        pve_virtual_machines,
        ceph_sources,
        kubernetes_sources,
        kubernetes_artifacts,
    )?;
    let source_names = DataSourceRepo::new(case_connection)
        .find_by_case(case_id)?
        .into_iter()
        .map(|source| (source.id.0, source.name))
        .collect::<BTreeMap<_, _>>();
    let member_sources = source_members
        .iter()
        .map(|(member_index, source_id)| {
            (
                *member_index,
                source_id.clone(),
                source_names.get(source_id).cloned().unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    project_environment_objects(
        case_connection,
        case_id,
        import_set_id,
        &projection,
        &member_sources,
    )?;
    Ok(projection)
}

#[allow(clippy::too_many_arguments)]
fn collect_source_observations(
    case_connection: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    import_set_id: &str,
    source_members: &[(u32, String)],
    scope_repo: &LinuxTopologyScopeRepo<'_>,
    membership_repo: &LinuxTopologyMembershipRepo<'_>,
    edge_repo: &LinuxTopologyEdgeRepo<'_>,
    projection: &mut ImportSetTopologyProjection,
) -> Result<SourceObservations> {
    let mut observations = SourceObservations {
        pve_sources: Vec::new(),
        pve_virtual_machines: BTreeMap::new(),
        ceph_sources: Vec::new(),
        kubernetes_sources: Vec::new(),
        kubernetes_artifacts: Vec::new(),
    };
    for (member_index, source_id) in source_members {
        let source = DataSourceId(source_id.clone());
        let source_connection =
            source_db::open_registered_source_db_read_only(case_connection, case_root, &source)?;
        register_host_and_os_scope(
            case_id,
            import_set_id,
            *member_index,
            source_id,
            scope_repo,
            membership_repo,
            edge_repo,
            projection,
        )?;
        observe_source(
            &source_connection,
            &source,
            *member_index,
            source_id,
            &mut observations,
        )?;
    }
    Ok(observations)
}

#[allow(clippy::too_many_arguments)]
fn register_host_and_os_scope(
    case_id: &CaseId,
    import_set_id: &str,
    member_index: u32,
    source_id: &str,
    scope_repo: &LinuxTopologyScopeRepo<'_>,
    membership_repo: &LinuxTopologyMembershipRepo<'_>,
    edge_repo: &LinuxTopologyEdgeRepo<'_>,
    projection: &mut ImportSetTopologyProjection,
) -> Result<()> {
    let host_id = format!("scope:physical-host:{import_set_id}:{member_index}");
    replace_scope(scope_repo, &host_id)?;
    scope_repo.insert(&scope_record(
        &host_id,
        case_id,
        TopologyScopeKind::PhysicalHost,
        format!("Physical host {member_index}"),
        "complete",
    ))?;
    membership_repo.insert(&membership_record(
        &host_id,
        source_id,
        TopologyMemberRole::Host,
        member_index,
        "catalog",
    ))?;
    let os_id = format!("scope:os:{import_set_id}:{member_index}");
    replace_scope(scope_repo, &os_id)?;
    scope_repo.insert(&scope_record(
        &os_id,
        case_id,
        TopologyScopeKind::OsInstance,
        format!("Linux host system {member_index}"),
        "complete",
    ))?;
    edge_repo.insert(&edge_record(
        &host_id,
        &os_id,
        TopologyEdgeKind::Hosts,
        "operating-system catalog from physical-host evidence",
    ))?;
    membership_repo.insert(&membership_record(
        &os_id,
        source_id,
        TopologyMemberRole::OsRoot,
        member_index,
        "catalog",
    ))?;
    projection.os_scope_ids.push(os_id);
    Ok(())
}

fn observe_source(
    source_connection: &rusqlite::Connection,
    source: &DataSourceId,
    member_index: u32,
    source_id: &str,
    observations: &mut SourceObservations,
) -> Result<()> {
    let entries = FileRepo::new(source_connection).find_by_data_source(source)?;
    if entries.iter().any(|entry| is_pve_path(&entry.path)) {
        observations
            .pve_sources
            .push((member_index, source_id.to_string()));
    }
    for entry in &entries {
        if let Some(vm_id) = pve_qemu_vm_id(&entry.path) {
            observations
                .pve_virtual_machines
                .entry(vm_id)
                .or_default()
                .push((member_index, source_id.to_string(), entry.path.clone()));
        }
    }
    if !CephOsdRepo::new(source_connection)
        .find_by_data_source(source_id)?
        .is_empty()
    {
        observations
            .ceph_sources
            .push((member_index, source_id.to_string()));
    }
    let inventory = discover_kubernetes_member_artifacts(source_connection, source)?;
    if !inventory.artifacts.is_empty() {
        let role = k8_member_role(&inventory.artifacts);
        observations
            .kubernetes_sources
            .push((member_index, source_id.to_string(), role));
        observations
            .kubernetes_artifacts
            .extend(inventory.artifacts);
    }
    Ok(())
}

fn k8_member_role(
    artifacts: &[super::kubernetes_inventory::KubernetesMemberArtifact],
) -> TopologyMemberRole {
    artifacts
        .iter()
        .any(|artifact| {
            matches!(
                artifact.kind,
                super::kubernetes_paths::KubernetesArtifactKind::StaticPodManifest
                    | super::kubernetes_paths::KubernetesArtifactKind::EtcdBackend
                    | super::kubernetes_paths::KubernetesArtifactKind::EtcdWal
            )
        })
        .then_some(TopologyMemberRole::ControlPlane)
        .unwrap_or(TopologyMemberRole::Unknown)
}

#[allow(clippy::too_many_arguments)]
fn register_detected_scopes(
    case_id: &CaseId,
    import_set_id: &str,
    scope_repo: &LinuxTopologyScopeRepo<'_>,
    membership_repo: &LinuxTopologyMembershipRepo<'_>,
    edge_repo: &LinuxTopologyEdgeRepo<'_>,
    artifact_repo: &LinuxTopologyArtifactRepo<'_>,
    storage_repo: &StorageObjectRepo<'_>,
    projection: &mut ImportSetTopologyProjection,
    pve_sources: Vec<(u32, String)>,
    pve_virtual_machines: BTreeMap<String, Vec<(u32, String, String)>>,
    ceph_sources: Vec<(u32, String)>,
    kubernetes_sources: Vec<(u32, String, TopologyMemberRole)>,
    kubernetes_artifacts: Vec<super::kubernetes_inventory::KubernetesMemberArtifact>,
) -> Result<()> {
    register_pve_scope(
        case_id,
        import_set_id,
        scope_repo,
        membership_repo,
        edge_repo,
        projection,
        pve_sources,
        pve_virtual_machines,
    )?;
    register_ceph_scope(
        case_id,
        import_set_id,
        scope_repo,
        membership_repo,
        storage_repo,
        projection,
        ceph_sources,
    )?;
    if !kubernetes_sources.is_empty() {
        let scope_id = format!("scope:kubernetes:{import_set_id}");
        replace_scope(&scope_repo, &scope_id)?;
        scope_repo.insert(&scope_record(
            &scope_id,
            case_id,
            TopologyScopeKind::Kubernetes,
            "Kubernetes candidate scope".to_string(),
            "partial",
        ))?;
        for (member_index, data_source_id, role) in &kubernetes_sources {
            membership_repo.insert(&membership_record(
                &scope_id,
                data_source_id,
                *role,
                *member_index,
                "Kubernetes artifact paths",
            ))?;
            let os_scope_id = format!("scope:os:{import_set_id}:{member_index}");
            edge_repo.insert(&edge_record(
                &os_scope_id,
                &scope_id,
                TopologyEdgeKind::Runs,
                "kubernetes artifact paths",
            ))?;
        }
        for artifact in kubernetes_artifacts {
            artifact_repo.insert(&LinuxTopologyArtifactRecord {
                scope_id: scope_id.clone(),
                data_source_id: artifact.data_source_id.0,
                file_id: Some(artifact.file_id.0),
                layer: "orchestration".to_string(),
                artifact_kind: artifact.kind.as_str().to_string(),
                parser: "kubernetes-path-detector".to_string(),
                status: "candidate_found".to_string(),
                diagnostics_json: "[]".to_string(),
                content_digest: None,
            })?;
        }
        projection.kubernetes_scope_id = Some(scope_id);
    }
    if let (Some(ceph_scope_id), Some(pve_scope_id)) =
        (&projection.ceph_scope_id, &projection.pve_scope_id)
    {
        edge_repo.insert(&edge_record(
            ceph_scope_id,
            pve_scope_id,
            TopologyEdgeKind::ProvidesStorage,
            "co-located PVE and Ceph evidence in one import set",
        ))?;
    }
    Ok(())
}

fn register_ceph_scope(
    case_id: &CaseId,
    import_set_id: &str,
    scope_repo: &LinuxTopologyScopeRepo<'_>,
    membership_repo: &LinuxTopologyMembershipRepo<'_>,
    storage_repo: &StorageObjectRepo<'_>,
    projection: &mut ImportSetTopologyProjection,
    ceph_sources: Vec<(u32, String)>,
) -> Result<()> {
    if ceph_sources.is_empty() {
        return Ok(());
    }
    let scope_id = format!("scope:ceph:{import_set_id}");
    replace_scope(scope_repo, &scope_id)?;
    scope_repo.insert(&scope_record(
        &scope_id,
        case_id,
        TopologyScopeKind::Ceph,
        "Ceph candidate scope".to_string(),
        "complete",
    ))?;
    insert_memberships(
        membership_repo,
        &scope_id,
        &ceph_sources,
        TopologyMemberRole::StorageNode,
    )?;
    storage_repo.insert_if_absent(&StorageObjectRecord {
        id: format!("storage:ceph:{import_set_id}"),
        case_id: case_id.0.clone(),
        object_kind: "ceph_cluster".to_string(),
        name: "Ceph storage cluster".to_string(),
        identity_state: "candidate".to_string(),
        status: "ready".to_string(),
        provenance_json: serde_json::json!({"scopeId": scope_id}).to_string(),
    })?;
    projection.ceph_scope_id = Some(scope_id);
    Ok(())
}
