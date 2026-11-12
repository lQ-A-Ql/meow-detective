use domain::CaseId;
use persistence_sqlite::repositories::{
    linux_topology_edge_repo::LinuxTopologyEdgeRecord,
    linux_topology_membership_repo::{LinuxTopologyMembershipRecord, LinuxTopologyMembershipRepo},
    linux_topology_scope_repo::{LinuxTopologyScopeRecord, LinuxTopologyScopeRepo},
};

use super::{Result, TopologyEdgeKind, TopologyMemberRole, TopologyScopeKind};

pub(super) fn replace_scope(scope_repo: &LinuxTopologyScopeRepo<'_>, scope_id: &str) -> Result<()> {
    scope_repo.delete(scope_id)?;
    Ok(())
}

pub(super) fn scope_record(
    id: &str,
    case_id: &CaseId,
    kind: TopologyScopeKind,
    name: String,
    completeness: &str,
) -> LinuxTopologyScopeRecord {
    LinuxTopologyScopeRecord {
        id: id.to_string(),
        case_id: case_id.0.clone(),
        scope_kind: kind.as_str().to_string(),
        name,
        identity_state: match kind {
            TopologyScopeKind::PhysicalHost => "unproven",
            _ => "candidate",
        }
        .to_string(),
        identity_fingerprint: None,
        status: scope_status(completeness).to_string(),
        evidence_completeness: completeness.to_string(),
        diagnostics_json: "[]".to_string(),
    }
}

fn scope_status(completeness: &str) -> &'static str {
    match completeness {
        "complete" => "ready",
        "partial" => "partial",
        _ => "discovered",
    }
}

pub(super) fn insert_memberships(
    repo: &LinuxTopologyMembershipRepo<'_>,
    scope_id: &str,
    sources: &[(u32, String)],
    role: TopologyMemberRole,
) -> Result<()> {
    for (member_index, data_source_id) in sources {
        repo.insert(&membership_record(
            scope_id,
            data_source_id,
            role,
            *member_index,
            "source-local artifact projection",
        ))?;
    }
    Ok(())
}

pub(super) fn membership_record(
    scope_id: &str,
    data_source_id: &str,
    role: TopologyMemberRole,
    member_index: u32,
    basis: &str,
) -> LinuxTopologyMembershipRecord {
    LinuxTopologyMembershipRecord {
        scope_id: scope_id.to_string(),
        data_source_id: data_source_id.to_string(),
        role: role.as_str().to_string(),
        member_index: Some(member_index),
        confidence: "candidate".to_string(),
        provenance_json: serde_json::json!({ "basis": basis }).to_string(),
    }
}

pub(super) fn edge_record(
    source_scope_id: &str,
    target_scope_id: &str,
    edge_kind: TopologyEdgeKind,
    basis: &str,
) -> LinuxTopologyEdgeRecord {
    LinuxTopologyEdgeRecord {
        source_scope_id: source_scope_id.to_string(),
        target_scope_id: target_scope_id.to_string(),
        edge_kind: edge_kind.as_str().to_string(),
        confidence: "candidate".to_string(),
        provenance_json: serde_json::json!({ "basis": basis }).to_string(),
    }
}

pub(super) fn is_pve_path(path: &str) -> bool {
    let path = path.replace('\\', "/").to_ascii_lowercase();
    path.contains("/etc/pve/")
        || path.ends_with("/etc/pve")
        || path.contains("/etc/corosync/corosync.conf")
}

pub(super) fn pve_qemu_vm_id(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let (_, file_name) = normalized.rsplit_once("/etc/pve/qemu-server/")?;
    let vm_id = file_name.strip_suffix(".conf")?;
    (!vm_id.is_empty() && vm_id.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| vm_id.to_string())
}
