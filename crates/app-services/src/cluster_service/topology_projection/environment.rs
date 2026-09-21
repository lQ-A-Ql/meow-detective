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
) -> Result<()> {
    let repo = EnvironmentObjectRepo::new(conn);
    for (index, os_scope_id) in projection.os_scope_ids.iter().enumerate() {
        let host_id = format!("env:host:{import_set_id}:{index}");
        let os_id = format!("env:os:{import_set_id}:{index}");
        insert_object(
            &repo,
            &host_id,
            case_id,
            "physical_host",
            &format!("Physical host {index}"),
            &format!("scope:physical-host:{import_set_id}:{index}"),
            "ready",
        )?;
        insert_object(
            &repo,
            &os_id,
            case_id,
            "os_instance",
            &format!("OS instance {index}"),
            os_scope_id,
            "ready",
        )?;
        repo.insert_relation(
            &host_id,
            &os_id,
            "hosts",
            "candidate",
            "{\"basis\":\"topology_projection\"}",
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
            "ready",
        )?;
        for index in 0..projection.os_scope_ids.len() {
            repo.insert_relation(
                &pve_id,
                &format!("env:host:{import_set_id}:{index}"),
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
            "partial",
        )?;
        for index in 0..projection.os_scope_ids.len() {
            repo.insert_relation(
                &format!("env:os:{import_set_id}:{index}"),
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
    status: &str,
) -> Result<()> {
    repo.insert_if_absent(&EnvironmentObjectRecord {
        id: id.to_string(),
        case_id: case_id.0.clone(),
        object_kind: kind.to_string(),
        name: name.to_string(),
        identity_state: "candidate".to_string(),
        status: status.to_string(),
        provenance_json: serde_json::json!({"scopeId": scope_id}).to_string(),
    })?;
    Ok(())
}
