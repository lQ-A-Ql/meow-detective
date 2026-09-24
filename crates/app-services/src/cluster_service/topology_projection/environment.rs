use domain::CaseId;
use persistence_sqlite::repositories::environment_object_repo::{
    EnvironmentObjectRecord, EnvironmentObjectRepo,
};

use super::ImportSetTopologyProjection;
use crate::cluster_service::Result;

pub(super) fn project_environment_objects(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    import_set_id: &str,
    projection: &ImportSetTopologyProjection,
    member_sources: &[(u32, String, String)],
) -> Result<()> {
    let repo = EnvironmentObjectRepo::new(conn);
    project_host_objects(&repo, case_id, import_set_id, projection, member_sources)?;
    if let Some(pve_scope_id) = &projection.pve_scope_id {
        let pve_id = format!("env:pve:{import_set_id}");
        insert_object(
            &repo,
            EnvironmentObjectSpec {
                id: &pve_id,
                case_id,
                kind: "pve",
                name: "PVE environment",
                scope_id: pve_scope_id,
                data_source_id: "",
                status: "partial",
            },
        )?;
        for (member_index, _, _) in member_sources {
            repo.insert_relation(
                &pve_id,
                &format!("env:host:{import_set_id}:{member_index}"),
                "manages",
                "candidate",
                "{\"basis\":\"pve_scope\"}",
            )?;
        }
    }
    for vm_scope_id in &projection.virtual_machine_scope_ids {
        let vm_id = format!(
            "env:vm:{import_set_id}:{}",
            vm_scope_id.rsplit(':').next().unwrap_or("unknown")
        );
        insert_object(
            &repo,
            EnvironmentObjectSpec {
                id: &vm_id,
                case_id,
                kind: "virtual_machine",
                name: "PVE virtual machine",
                scope_id: vm_scope_id,
                data_source_id: "",
                status: "partial",
            },
        )?;
    }
    if let Some(kubernetes_scope_id) = &projection.kubernetes_scope_id {
        let kubernetes_id = format!("env:kubernetes:{import_set_id}");
        insert_object(
            &repo,
            EnvironmentObjectSpec {
                id: &kubernetes_id,
                case_id,
                kind: "kubernetes",
                name: "Kubernetes environment",
                scope_id: kubernetes_scope_id,
                data_source_id: "",
                status: "partial",
            },
        )?;
        for (member_index, _, _) in member_sources {
            repo.insert_relation(
                &format!("env:os:{import_set_id}:{member_index}"),
                &kubernetes_id,
                "runs",
                "candidate",
                "{\"basis\":\"kubernetes_scope\"}",
            )?;
        }
    }
    Ok(())
}

fn project_host_objects(
    repo: &EnvironmentObjectRepo<'_>,
    case_id: &CaseId,
    import_set_id: &str,
    projection: &ImportSetTopologyProjection,
    member_sources: &[(u32, String, String)],
) -> Result<()> {
    for ((member_index, data_source_id, source_name), os_scope_id) in
        member_sources.iter().zip(&projection.os_scope_ids)
    {
        let source_name = if source_name.is_empty() {
            format!("member-{}", member_index + 1)
        } else {
            source_name.clone()
        };
        let host_id = format!("env:host:{import_set_id}:{member_index}");
        let os_id = format!("env:os:{import_set_id}:{member_index}");
        let host_name = format!("Host evidence: {source_name}");
        let host_scope_id = format!("scope:physical-host:{import_set_id}:{member_index}");
        insert_object(
            repo,
            EnvironmentObjectSpec {
                id: &host_id,
                case_id,
                kind: "physical_host",
                name: &host_name,
                scope_id: &host_scope_id,
                data_source_id,
                status: "partial",
            },
        )?;
        let os_name = format!("Linux system: {source_name}");
        insert_object(
            repo,
            EnvironmentObjectSpec {
                id: &os_id,
                case_id,
                kind: "os_instance",
                name: &os_name,
                scope_id: os_scope_id,
                data_source_id,
                status: "partial",
            },
        )?;
        repo.insert_relation(
            &host_id,
            &os_id,
            "hosts",
            "candidate",
            &serde_json::json!({ "basis": "topology_projection", "dataSourceId": data_source_id })
                .to_string(),
        )?;
    }
    Ok(())
}

struct EnvironmentObjectSpec<'a> {
    id: &'a str,
    case_id: &'a CaseId,
    kind: &'a str,
    name: &'a str,
    scope_id: &'a str,
    data_source_id: &'a str,
    status: &'a str,
}

fn insert_object(repo: &EnvironmentObjectRepo<'_>, spec: EnvironmentObjectSpec<'_>) -> Result<()> {
    repo.upsert(&EnvironmentObjectRecord {
        id: spec.id.to_string(),
        case_id: spec.case_id.0.clone(),
        object_kind: spec.kind.to_string(),
        name: spec.name.to_string(),
        identity_state: "candidate".to_string(),
        status: spec.status.to_string(),
        provenance_json: serde_json::json!({
            "scopeId": spec.scope_id,
            "dataSourceId": spec.data_source_id
        })
        .to_string(),
    })?;
    Ok(())
}
