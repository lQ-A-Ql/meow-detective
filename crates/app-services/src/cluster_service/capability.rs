use domain::{CephScopeId, KubernetesScopeId, OsInstanceId};
use persistence_sqlite::repositories::linux_topology_scope_repo::{
    LinuxTopologyScopeRepo, LinuxTopologyScopeSummary,
};
use rusqlite::Connection;

use super::{ClusterServiceError, Result, TopologyScopeKind};

pub fn require_ceph_scope(
    connection: &Connection,
    case_id: &str,
    scope_id: &CephScopeId,
) -> Result<LinuxTopologyScopeSummary> {
    require_scope(connection, case_id, &scope_id.0, TopologyScopeKind::Ceph)
}

pub fn require_kubernetes_scope(
    connection: &Connection,
    case_id: &str,
    scope_id: &KubernetesScopeId,
) -> Result<LinuxTopologyScopeSummary> {
    require_scope(
        connection,
        case_id,
        &scope_id.0,
        TopologyScopeKind::Kubernetes,
    )
}

pub fn require_os_scope(
    connection: &Connection,
    case_id: &str,
    scope_id: &OsInstanceId,
) -> Result<LinuxTopologyScopeSummary> {
    require_scope(
        connection,
        case_id,
        &scope_id.0,
        TopologyScopeKind::OsInstance,
    )
}

fn require_scope(
    connection: &Connection,
    case_id: &str,
    scope_id: &str,
    expected_kind: TopologyScopeKind,
) -> Result<LinuxTopologyScopeSummary> {
    let repo = LinuxTopologyScopeRepo::new(connection);
    let kind = repo
        .find_kind(scope_id)?
        .ok_or(ClusterServiceError::InvalidClusterId)?;
    if kind != expected_kind.as_str() {
        return Err(ClusterServiceError::Unsupported);
    }
    let summary = repo.find_summary(case_id, scope_id)?;
    summary.ok_or(ClusterServiceError::InvalidClusterId)
}
