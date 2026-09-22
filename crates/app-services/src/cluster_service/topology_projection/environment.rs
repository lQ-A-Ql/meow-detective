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
    for ((member_index, data_source_id, source_name), os_scope_id) in
        member_sources.iter().zip(&projection.os_scope_ids)
    {
        let source_name = (!source_name.is_empty())
            .then(|| source_name.clone())
            .unwrap_or_else(|| format!("member-{}", member_index + 1));
        let host_id = format!("env:host:{import_set_id}:{member_index}");
        let os_id = format!("env:os:{import_set_id}:{member_index}");
        insert_object(
            &repo,
            &host_id,
            case_id,
            "physical_host",
            &format!("Host evidence: {source_name}"),
            &format!("scope:physical-host:{import_set_id}:{member_index}"),
            data_source_id,
            "ready",
        )?;
        insert_object(
            &repo,
            &os_id,
            case_id,
            "os_instance",
            &format!("Linux system: {source_name}"),
            os_scope_id,
            data_source_id,
            "ready",
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
    if let Some(pve_scope_id) = &projection.pve_scope_id {
        let pve_id = format!("env:pve:{import_set_id}");
        insert_object(
            &repo,
            &pve_id,
            case_id,
            "pve",
            "PVE environment",
            pve_scope_id,
            "",
            "ready",
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
            &vm_id,
            case_id,
            "virtual_machine",
            "PVE virtual machine",
            vm_scope_id,
            "",
            "partial",
        )?;
    }
    if let Some(kubernetes_scope_id) = &projection.kubernetes_scope_id {
        let kubernetes_id = format!("env:kubernetes:{import_set_id}");
        insert_object(
            &repo,
            &kubernetes_id,
            case_id,
            "kubernetes",
            "Kubernetes environment",
            kubernetes_scope_id,
            "",
            "partial",
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

fn insert_object(
    repo: &EnvironmentObjectRepo<'_>,
    id: &str,
    case_id: &CaseId,
    kind: &str,
    name: &str,
    scope_id: &str,
    data_source_id: &str,
    status: &str,
) -> Result<()> {
    repo.upsert(&EnvironmentObjectRecord {
        id: id.to_string(),
        case_id: case_id.0.clone(),
        object_kind: kind.to_string(),
        name: name.to_string(),
        identity_state: "candidate".to_string(),
        status: status.to_string(),
        provenance_json: serde_json::json!({"scopeId": scope_id, "dataSourceId": data_source_id})
            .to_string(),
    })?;
    Ok(())
}
