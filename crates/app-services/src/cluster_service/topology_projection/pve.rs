use std::collections::BTreeMap;

use domain::CaseId;
use persistence_sqlite::repositories::{
    linux_topology_edge_repo::LinuxTopologyEdgeRepo,
    linux_topology_membership_repo::LinuxTopologyMembershipRepo,
    linux_topology_scope_repo::LinuxTopologyScopeRepo,
};

use super::{
    edge_record, insert_memberships, membership_record, replace_scope, scope_record,
    ImportSetTopologyProjection,
};
use crate::cluster_service::{Result, TopologyEdgeKind, TopologyMemberRole, TopologyScopeKind};

#[allow(clippy::too_many_arguments)]
pub(super) fn register_pve_scope(
    case_id: &CaseId,
    import_set_id: &str,
    scope_repo: &LinuxTopologyScopeRepo<'_>,
    membership_repo: &LinuxTopologyMembershipRepo<'_>,
    edge_repo: &LinuxTopologyEdgeRepo<'_>,
    projection: &mut ImportSetTopologyProjection,
    pve_sources: Vec<(u32, String)>,
    virtual_machines: BTreeMap<String, Vec<(u32, String, String)>>,
) -> Result<()> {
    if pve_sources.is_empty() {
        return Ok(());
    }
    let scope_id = format!("scope:pve:{import_set_id}");
    replace_scope(scope_repo, &scope_id)?;
    scope_repo.insert(&scope_record(
        &scope_id,
        case_id,
        TopologyScopeKind::Pve,
        "PVE candidate scope".to_string(),
        "partial",
    ))?;
    insert_memberships(
        membership_repo,
        &scope_id,
        &pve_sources,
        TopologyMemberRole::Host,
    )?;
    for (member_index, _) in &pve_sources {
        edge_repo.insert(&edge_record(
            &scope_id,
            &format!("scope:physical-host:{import_set_id}:{member_index}"),
            TopologyEdgeKind::Manages,
            "PVE configuration evidence",
        ))?;
    }
    for (vm_id, evidence) in virtual_machines {
        register_virtual_machine(
            case_id,
            import_set_id,
            scope_repo,
            membership_repo,
            edge_repo,
            projection,
            &scope_id,
            vm_id,
            evidence,
        )?;
    }
    projection.pve_scope_id = Some(scope_id);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn register_virtual_machine(
    case_id: &CaseId,
    import_set_id: &str,
    scope_repo: &LinuxTopologyScopeRepo<'_>,
    membership_repo: &LinuxTopologyMembershipRepo<'_>,
    edge_repo: &LinuxTopologyEdgeRepo<'_>,
    projection: &mut ImportSetTopologyProjection,
    pve_scope_id: &str,
    vm_id: String,
    evidence: Vec<(u32, String, String)>,
) -> Result<()> {
    let vm_scope_id = format!("scope:vm:{import_set_id}:{vm_id}");
    replace_scope(scope_repo, &vm_scope_id)?;
    scope_repo.insert(&scope_record(
        &vm_scope_id,
        case_id,
        TopologyScopeKind::VirtualMachine,
        format!("PVE virtual machine {vm_id}"),
        "partial",
    ))?;
    for (member_index, source_id, path) in evidence {
        membership_repo.insert(&membership_record(
            &vm_scope_id,
            &source_id,
            TopologyMemberRole::VirtualMachine,
            member_index,
            &format!("PVE VM configuration: {path}"),
        ))?;
    }
    edge_repo.insert(&edge_record(
        pve_scope_id,
        &vm_scope_id,
        TopologyEdgeKind::Hosts,
        "PVE QEMU virtual-machine configuration",
    ))?;
    projection.virtual_machine_scope_ids.push(vm_scope_id);
    Ok(())
}
