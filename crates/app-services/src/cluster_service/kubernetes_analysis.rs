use std::collections::HashMap;

use domain::{CaseId, DataSourceId, KubernetesScopeId};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, linux_topology_scope_repo::LinuxTopologyScopeRepo,
};
use rusqlite::Connection;
use transport::dto::{
    AnalysisParseStatusDto, KubernetesClusterArtifactDto, KubernetesClusterNodeDto,
    KubernetesClusterSummaryDto,
};

use super::kubernetes_inventory::discover_kubernetes_cluster_artifacts;
use super::{Result, TopologyScopeKind};

pub fn get_source_kubernetes_cluster_summary(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<KubernetesClusterSummaryDto> {
    let scope_repo = LinuxTopologyScopeRepo::new(case_connection);
    let Some(scope) = scope_repo.find_for_source(
        &case_id.0,
        &data_source_id.0,
        TopologyScopeKind::Kubernetes.as_str(),
    )?
    else {
        return Ok(not_found_summary(data_source_id));
    };

    let sources = DataSourceRepo::new(case_connection).find_by_case(case_id)?;
    let source_names = sources
        .into_iter()
        .map(|source| (source.id.0, source.name))
        .collect::<HashMap<_, _>>();
    let inventory = discover_kubernetes_cluster_artifacts(
        case_connection,
        case_root,
        case_id,
        &KubernetesScopeId(scope.id.clone()),
    )?;
    let mut nodes = Vec::with_capacity(inventory.members.len());
    let mut artifacts = Vec::new();
    let mut control_plane_member_count = 0u32;
    for member in inventory.members {
        let (node, member_artifacts) = summarize_member(member, &source_names);
        control_plane_member_count += u32::from(node.control_plane);
        nodes.push(node);
        artifacts.extend(member_artifacts);
    }
    let artifact_count = artifacts.len() as u64;
    let mut diagnostics = vec![
        "cluster identity is not yet proven from etcd cluster ID and PKI fingerprints".to_string(),
    ];
    if scope.status != "ready" {
        diagnostics.push(format!(
            "Kubernetes scope status is {}; member identity remains candidate",
            scope.status
        ));
    }
    Ok(KubernetesClusterSummaryDto {
        status: if artifact_count > 0 {
            AnalysisParseStatusDto::CandidateFound
        } else {
            AnalysisParseStatusDto::NotFound
        },
        scope_id: Some(scope.id),
        scope_name: Some(scope.name),
        selected_data_source_id: data_source_id.0.clone(),
        expected_member_count: scope.member_count,
        ready_member_count: if scope.status == "ready" {
            scope.member_count
        } else {
            0
        },
        control_plane_member_count,
        artifact_count,
        nodes,
        artifacts,
        diagnostics,
    })
}

fn summarize_member(
    member: super::kubernetes_inventory::KubernetesMemberArtifactInventory,
    source_names: &HashMap<String, String>,
) -> (KubernetesClusterNodeDto, Vec<KubernetesClusterArtifactDto>) {
    let control_plane = is_control_plane_candidate(&member.artifacts);
    let artifact_count = member.artifacts.len() as u64;
    let source_name = source_names
        .get(&member.data_source_id.0)
        .cloned()
        .unwrap_or_else(|| member.data_source_id.0.clone());
    let diagnostics = if control_plane {
        vec!["control-plane evidence is present; node identity is not yet proven".to_string()]
    } else {
        vec!["no control-plane evidence was discovered in this source".to_string()]
    };
    let node = KubernetesClusterNodeDto {
        data_source_id: member.data_source_id.0.clone(),
        source_name,
        node_name: None,
        control_plane,
        artifact_count,
        parsed_artifact_count: 0,
        failed_artifact_count: 0,
        status: if artifact_count > 0 {
            AnalysisParseStatusDto::CandidateFound
        } else {
            AnalysisParseStatusDto::NotFound
        },
        diagnostics,
    };
    let artifacts = member
        .artifacts
        .into_iter()
        .map(|artifact| KubernetesClusterArtifactDto {
            data_source_id: artifact.data_source_id.0,
            file_id: artifact.file_id.0,
            path: artifact.path,
            kind: artifact.kind.as_str().to_string(),
            size: artifact.size,
            deleted: artifact.deleted,
            encrypted: artifact.encrypted,
            status: AnalysisParseStatusDto::CandidateFound,
            detail: Some("candidate discovered; content parser is available on demand".to_string()),
            diagnostics: Vec::new(),
        })
        .collect();
    (node, artifacts)
}

fn is_control_plane_candidate(
    artifacts: &[super::kubernetes_inventory::KubernetesMemberArtifact],
) -> bool {
    artifacts.iter().any(|artifact| {
        matches!(
            artifact.kind,
            super::kubernetes_paths::KubernetesArtifactKind::StaticPodManifest
                | super::kubernetes_paths::KubernetesArtifactKind::EtcdBackend
                | super::kubernetes_paths::KubernetesArtifactKind::EtcdWal
        )
    })
}

fn not_found_summary(data_source_id: &DataSourceId) -> KubernetesClusterSummaryDto {
    KubernetesClusterSummaryDto {
        status: AnalysisParseStatusDto::NotFound,
        scope_id: None,
        scope_name: None,
        selected_data_source_id: data_source_id.0.clone(),
        expected_member_count: 0,
        ready_member_count: 0,
        control_plane_member_count: 0,
        artifact_count: 0,
        nodes: Vec::new(),
        artifacts: Vec::new(),
        diagnostics: vec!["data source is not assigned to a Kubernetes cluster".to_string()],
    }
}
