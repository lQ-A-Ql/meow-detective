use std::collections::HashMap;

use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::{
    datasource_cluster_repo::DataSourceClusterRepo, datasource_repo::DataSourceRepo,
};
use rusqlite::Connection;
use transport::dto::{
    AnalysisParseStatusDto, KubernetesClusterArtifactDto, KubernetesClusterNodeDto,
    KubernetesClusterSummaryDto,
};

use super::kubernetes_inventory::discover_kubernetes_cluster_artifacts;
use super::{ClusterServiceError, Result, KUBERNETES_CLUSTER_PROFILE};

pub fn get_source_kubernetes_cluster_summary(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<KubernetesClusterSummaryDto> {
    let cluster_id =
        DataSourceRepo::new(case_connection).find_cluster_id_by_source(case_id, data_source_id)?;
    let Some(cluster_id) = cluster_id else {
        return Ok(not_found_summary(data_source_id));
    };
    let cluster = DataSourceClusterRepo::new(case_connection)
        .find_by_id(&cluster_id)?
        .ok_or(ClusterServiceError::InvalidClusterId)?;
    if cluster.case_id != *case_id || cluster.profile.as_deref() != Some(KUBERNETES_CLUSTER_PROFILE)
    {
        return Ok(not_found_summary(data_source_id));
    }

    let sources = DataSourceRepo::new(case_connection).find_by_case(case_id)?;
    let source_names = sources
        .into_iter()
        .map(|source| (source.id.0, source.name))
        .collect::<HashMap<_, _>>();
    let inventory =
        discover_kubernetes_cluster_artifacts(case_connection, case_root, case_id, &cluster_id)?;
    let mut nodes = Vec::with_capacity(inventory.members.len());
    let mut artifacts = Vec::new();
    let mut control_plane_member_count = 0u32;
    for member in inventory.members {
        let source_name = source_names
            .get(&member.data_source_id.0)
            .cloned()
            .unwrap_or_else(|| member.data_source_id.0.clone());
        let control_plane = member.artifacts.iter().any(|artifact| {
            matches!(
                artifact.kind,
                super::kubernetes_paths::KubernetesArtifactKind::StaticPodManifest
                    | super::kubernetes_paths::KubernetesArtifactKind::Kubeconfig
                    | super::kubernetes_paths::KubernetesArtifactKind::EtcdBackend
                    | super::kubernetes_paths::KubernetesArtifactKind::EtcdWal
            )
        });
        if control_plane {
            control_plane_member_count += 1;
        }
        let node_artifact_count = member.artifacts.len() as u64;
        let node_diagnostics = if control_plane {
            vec!["control-plane evidence is present; node identity is not yet proven".to_string()]
        } else {
            vec!["no control-plane evidence was discovered in this source".to_string()]
        };
        nodes.push(KubernetesClusterNodeDto {
            data_source_id: member.data_source_id.0.clone(),
            source_name,
            node_name: None,
            control_plane,
            artifact_count: node_artifact_count,
            parsed_artifact_count: 0,
            failed_artifact_count: 0,
            status: if node_artifact_count > 0 {
                AnalysisParseStatusDto::CandidateFound
            } else {
                AnalysisParseStatusDto::NotFound
            },
            diagnostics: node_diagnostics,
        });
        artifacts.extend(member.artifacts.into_iter().map(|artifact| {
            KubernetesClusterArtifactDto {
                data_source_id: artifact.data_source_id.0,
                file_id: artifact.file_id.0,
                path: artifact.path,
                kind: artifact.kind.as_str().to_string(),
                size: artifact.size,
                deleted: artifact.deleted,
                encrypted: artifact.encrypted,
                status: AnalysisParseStatusDto::CandidateFound,
                detail: Some(
                    "candidate discovered; content parser is available on demand".to_string(),
                ),
                diagnostics: Vec::new(),
            }
        }));
    }
    let artifact_count = artifacts.len() as u64;
    let mut diagnostics = vec![
        "cluster identity is not yet proven from etcd cluster ID and PKI fingerprints".to_string(),
    ];
    if cluster.ready_count < cluster.member_count {
        diagnostics.push(format!(
            "{} of {} cluster members are ready",
            cluster.ready_count, cluster.member_count
        ));
    }
    Ok(KubernetesClusterSummaryDto {
        status: if artifact_count > 0 {
            AnalysisParseStatusDto::CandidateFound
        } else {
            AnalysisParseStatusDto::NotFound
        },
        cluster_id: Some(cluster.id),
        cluster_name: Some(cluster.name),
        selected_data_source_id: data_source_id.0.clone(),
        expected_member_count: cluster.member_count,
        ready_member_count: cluster.ready_count,
        control_plane_member_count,
        artifact_count,
        nodes,
        artifacts,
        diagnostics,
    })
}

fn not_found_summary(data_source_id: &DataSourceId) -> KubernetesClusterSummaryDto {
    KubernetesClusterSummaryDto {
        status: AnalysisParseStatusDto::NotFound,
        cluster_id: None,
        cluster_name: None,
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
