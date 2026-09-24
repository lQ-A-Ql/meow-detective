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
use super::{parse_kubernetes_artifact, Result, TopologyScopeKind};

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

pub fn run_kubernetes_cluster_analysis(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    import_set_id: &str,
) -> Result<transport::dto::KubernetesAnalysisRunDto> {
    let scope = LinuxTopologyScopeRepo::new(case_connection)
        .find_for_import_set(
            &case_id.0,
            import_set_id,
            TopologyScopeKind::Kubernetes.as_str(),
        )?
        .ok_or(super::ClusterServiceError::InvalidClusterId)?;
    let source_ids =
        DataSourceRepo::new(case_connection).find_ids_by_topology_scope(case_id, &scope.id)?;
    let expected_member_count = source_ids.len() as u32;
    let run_id = uuid::Uuid::new_v4().to_string();
    let started_at = chrono::Utc::now().to_rfc3339();
    let attempt: u32 = case_connection
        .query_row(
            "SELECT COALESCE(MAX(attempt), 0) + 1 FROM kubernetes_analysis_runs
             WHERE case_id = ?1 AND import_set_id = ?2 AND scope_id = ?3",
            rusqlite::params![case_id.0, import_set_id, scope.id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(1)
        .max(1) as u32;
    case_connection.execute(
        "INSERT INTO kubernetes_analysis_runs
         (id, case_id, import_set_id, scope_id, state, attempt, expected_member_count, started_at)
         VALUES (?1, ?2, ?3, ?4, 'running', ?5, ?6, ?7)",
        rusqlite::params![
            run_id,
            case_id.0,
            import_set_id,
            scope.id,
            attempt,
            source_ids.len() as u32,
            started_at
        ],
    )?;

    let (parsed_count, failed_count, diagnostics) =
        process_kubernetes_sources(case_connection, case_root, case_id, source_ids, &run_id)?;
    let state = if failed_count > 0 {
        "failed"
    } else {
        "completed"
    };
    let finished_at = chrono::Utc::now().to_rfc3339();
    case_connection.execute(
        "UPDATE kubernetes_analysis_runs
         SET state = ?1, parsed_artifact_count = ?2, failed_artifact_count = ?3,
             diagnostics_json = ?4, finished_at = ?5 WHERE id = ?6",
        rusqlite::params![
            state,
            parsed_count,
            failed_count,
            serde_json::to_string(&diagnostics)?,
            finished_at,
            run_id
        ],
    )?;
    Ok(transport::dto::KubernetesAnalysisRunDto {
        run_id,
        import_set_id: import_set_id.to_string(),
        scope_id: scope.id,
        state: state.to_string(),
        attempt,
        expected_member_count,
        parsed_artifact_count: parsed_count,
        failed_artifact_count: failed_count,
        diagnostics,
        started_at,
        finished_at: Some(finished_at),
    })
}

fn process_kubernetes_sources(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    source_ids: Vec<DataSourceId>,
    run_id: &str,
) -> Result<(u32, u32, Vec<String>)> {
    let mut parsed_count = 0u32;
    let mut failed_count = 0u32;
    let mut diagnostics = Vec::new();
    for source_id in source_ids {
        let source = crate::source_db::open_ready_source_read_only_by_id(
            case_connection,
            case_root,
            case_id,
            &source_id,
        )
        .map_err(|error| super::ClusterServiceError::Db(error.into_db_error()))?;
        let inventory =
            super::discover_kubernetes_member_artifacts(&source.connection, &source_id)?;
        let mut read_context = crate::file_service::SourceReadContext::new(
            &source.connection,
            case_connection,
            case_root,
            case_id,
            &source_id,
        );
        for artifact in inventory.artifacts {
            let max_bytes = artifact.size.min(16 * 1024 * 1024) as u32;
            let parsed = crate::file_service::read_file_bytes_for_case(
                &mut read_context,
                &artifact.file_id,
                0,
                max_bytes,
            )
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                let digest = infrastructure::hashing::sha256_bytes(&bytes);
                parse_kubernetes_artifact(artifact.kind, &bytes)
                    .map(|parsed| (parsed, digest))
                    .map_err(|error| error.to_string())
            });
            let (status, diagnostics_json, digest) = match parsed {
                Ok((Some(_), digest)) => {
                    parsed_count = parsed_count.saturating_add(1);
                    ("parsed", "[]".to_string(), Some(digest))
                }
                Ok((None, digest)) => ("candidate", "[]".to_string(), Some(digest)),
                Err(error) => {
                    failed_count = failed_count.saturating_add(1);
                    diagnostics.push(format!("{}: {}", artifact.path, error));
                    ("failed", serde_json::json!([error]).to_string(), None)
                }
            };
            case_connection.execute(
                "INSERT OR REPLACE INTO kubernetes_analysis_artifacts
                 (run_id, data_source_id, file_id, kind, status, diagnostics_json, content_digest)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    run_id,
                    source_id.0,
                    artifact.file_id.0,
                    artifact.kind.as_str(),
                    status,
                    diagnostics_json,
                    digest
                ],
            )?;
        }
    }
    Ok((parsed_count, failed_count, diagnostics))
}
