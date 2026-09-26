use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use domain::{CaseId, DataSourceKind};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use transport::dto::{
    LinuxDerivedSourceSummaryDto, LinuxEvidenceSetListItemDto, LinuxEvidenceSetMemberSummaryDto,
    LinuxEvidenceSetSummaryDto, LinuxTopologyEdgeSummaryDto, LinuxTopologyMemberSummaryDto,
    LinuxTopologyScopeSummaryDto,
};

use super::linux_evidence_facts::collect_linux_evidence_facts;
use super::{ClusterServiceError, Result};

#[derive(Debug, Clone)]
struct ImportSetRow {
    id: String,
    name: String,
    root_path: String,
    state: String,
    member_count: u32,
    ready_count: u32,
    failed_count: u32,
}

#[derive(Debug, Clone)]
struct ScopeRow {
    id: String,
    kind: String,
    name: String,
    identity_state: String,
    status: String,
    completeness: String,
    diagnostics: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestProvenance {
    schema_version: u32,
    collected_at: String,
    manifest_digest: String,
}

pub fn get_linux_evidence_set_summary(
    conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    import_set_id: &str,
) -> Result<LinuxEvidenceSetSummaryDto> {
    let import_set = load_import_set(conn, case_id, import_set_id)?;
    let manifest = load_manifest_provenance(case_root, import_set_id);
    let sources = DataSourceRepo::new(conn)
        .find_by_case(case_id)?
        .into_iter()
        .map(|source| (source.id.0.clone(), source))
        .collect::<BTreeMap<_, _>>();
    let members = load_members(
        conn,
        case_root,
        case_id,
        import_set_id,
        &import_set.root_path,
        &sources,
    )?;
    let scopes = load_scopes(conn, case_id, import_set_id)?;
    let scope_ids = scopes
        .iter()
        .map(|scope| scope.id.clone())
        .collect::<BTreeSet<_>>();
    let edges = load_edges(conn, &scope_ids)?;
    let derived_sources = load_derived_sources(conn, &sources);
    let mut diagnostics = Vec::new();
    if import_set.state != "ready" {
        diagnostics.push(format!("import set state is {}", import_set.state));
    }
    if manifest.is_none() {
        diagnostics.push("linuxEvidenceSet manifest is missing or invalid".to_string());
    }
    if members.iter().any(|member| member.hash_status != "hashed") {
        diagnostics.push("one or more members lack completed source hash verification".to_string());
    }
    if scopes
        .iter()
        .any(|scope| scope.evidence_completeness != "complete")
    {
        diagnostics.push("topology scopes are partial or indeterminate".to_string());
    }
    if let Some(ceph_scope) = scopes.iter().find(|scope| scope.kind == "ceph") {
        match crate::ceph_reconstruction::evidence_is_present(case_root, &ceph_scope.id) {
            Ok(true) => diagnostics.push("OSDMap/CRUSH evidence file is present".to_string()),
            Ok(false) => diagnostics.push(
                "OSDMap/CRUSH evidence is absent; RBD placement and replica closure are not proven"
                    .to_string(),
            ),
            Err(error) => diagnostics.push(format!(
                "OSDMap/CRUSH evidence could not be checked: {error}"
            )),
        }
    }
    let capability_level = capability_level(&import_set, &derived_sources);
    Ok(LinuxEvidenceSetSummaryDto {
        import_set_id: import_set.id,
        name: import_set.name,
        state: import_set.state,
        member_count: import_set.member_count,
        ready_count: import_set.ready_count,
        failed_count: import_set.failed_count,
        capability_level,
        manifest_digest: manifest.as_ref().map(|value| value.manifest_digest.clone()),
        manifest_schema_version: manifest.as_ref().map(|value| value.schema_version),
        collected_at: manifest.as_ref().map(|value| value.collected_at.clone()),
        members,
        scopes,
        edges,
        derived_sources,
        diagnostics,
    })
}

fn load_manifest_provenance(case_root: &Path, import_set_id: &str) -> Option<ManifestProvenance> {
    let path = case_root.join(format!(
        "import-sets/{import_set_id}/import-set-manifest.json"
    ));
    let payload = std::fs::read(path).ok()?;
    let manifest = serde_json::from_slice::<ManifestProvenance>(&payload).ok()?;
    (manifest.schema_version >= 2 && !manifest.manifest_digest.is_empty()).then_some(manifest)
}

pub fn list_linux_evidence_sets(
    conn: &Connection,
    case_id: &CaseId,
) -> Result<Vec<LinuxEvidenceSetListItemDto>> {
    let mut statement = conn.prepare(
        "SELECT id, name, import_state, member_count, ready_count, failed_count
         FROM linux_import_sets WHERE case_id = ?1 ORDER BY updated_at DESC, id DESC",
    )?;
    let rows = statement.query_map([&case_id.0], |row| {
        Ok(LinuxEvidenceSetListItemDto {
            import_set_id: row.get(0)?,
            name: row.get(1)?,
            state: row.get(2)?,
            member_count: row.get::<_, i64>(3)?.max(0) as u32,
            ready_count: row.get::<_, i64>(4)?.max(0) as u32,
            failed_count: row.get::<_, i64>(5)?.max(0) as u32,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn load_import_set(
    conn: &Connection,
    case_id: &CaseId,
    import_set_id: &str,
) -> Result<ImportSetRow> {
    conn.query_row(
        "SELECT id, name, root_path, import_state, member_count, ready_count, failed_count
         FROM linux_import_sets WHERE id = ?1 AND case_id = ?2",
        params![import_set_id, case_id.0],
        |row| {
            Ok(ImportSetRow {
                id: row.get(0)?,
                name: row.get(1)?,
                root_path: row.get(2)?,
                state: row.get(3)?,
                member_count: row.get::<_, i64>(4)?.max(0) as u32,
                ready_count: row.get::<_, i64>(5)?.max(0) as u32,
                failed_count: row.get::<_, i64>(6)?.max(0) as u32,
            })
        },
    )
    .optional()?
    .ok_or(ClusterServiceError::InvalidClusterId)
}

fn load_members(
    conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    import_set_id: &str,
    root_path: &str,
    sources: &BTreeMap<String, domain::DataSource>,
) -> Result<Vec<LinuxEvidenceSetMemberSummaryDto>> {
    let mut statement = conn.prepare(
        "SELECT member_index, source_path, source_kind, data_source_id, import_state
         FROM linux_import_set_members WHERE import_set_id = ?1 ORDER BY member_index",
    )?;
    let rows = statement
        .query_map([import_set_id], |row| {
            Ok((
                row.get::<_, i64>(0)?.max(0) as u32,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(statement);
    rows.into_iter()
        .map(
            |(member_index, source_path, source_kind, source_id, import_state)| {
                let source = source_id.as_ref().and_then(|id| sources.get(id));
                let facts = source_id.as_ref().map(|source_id| {
                    collect_linux_evidence_facts(
                        conn,
                        case_root,
                        case_id,
                        &domain::DataSourceId(source_id.clone()),
                    )
                });
                Ok(LinuxEvidenceSetMemberSummaryDto {
                    member_index,
                    data_source_id: source_id,
                    source_name: source
                        .map(|value| value.name.clone())
                        .unwrap_or_else(|| format!("member-{}", member_index + 1)),
                    source_path: root_relative_path(root_path, &source_path),
                    source_kind,
                    import_state,
                    hash_status: source
                        .map(|value| {
                            format!("{:?}", value.provenance.hash_status).to_ascii_lowercase()
                        })
                        .unwrap_or_else(|| "unknown".to_string()),
                    provenance_status: source
                        .map(|value| {
                            format!("{:?}", value.provenance.provenance_status).to_ascii_lowercase()
                        })
                        .unwrap_or_else(|| "unknown".to_string()),
                    hostname: facts.as_ref().and_then(|facts| facts.hostname.clone()),
                    operating_system: facts
                        .as_ref()
                        .and_then(|facts| facts.operating_system.clone()),
                    os_version: facts.as_ref().and_then(|facts| facts.os_version.clone()),
                    kernel_version: facts
                        .as_ref()
                        .and_then(|facts| facts.kernel_version.clone()),
                    addresses: facts
                        .as_ref()
                        .map(|facts| facts.addresses.clone())
                        .unwrap_or_default(),
                    roles: facts
                        .as_ref()
                        .map(|facts| facts.roles.clone())
                        .unwrap_or_default(),
                    services: facts
                        .as_ref()
                        .map(|facts| facts.services.clone())
                        .unwrap_or_default(),
                    containers: facts
                        .as_ref()
                        .map(|facts| facts.containers.clone())
                        .unwrap_or_default(),
                    diagnostics: facts.map(|facts| facts.diagnostics).unwrap_or_default(),
                })
            },
        )
        .collect::<Result<Vec<_>>>()
        .map_err(Into::into)
}

fn root_relative_path(root_path: &str, source_path: &str) -> String {
    let root = std::path::Path::new(root_path);
    let source = std::path::Path::new(source_path);
    source
        .strip_prefix(root)
        .ok()
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| {
            source
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "member".to_string())
        })
}

fn load_scopes(
    conn: &Connection,
    case_id: &CaseId,
    import_set_id: &str,
) -> Result<Vec<LinuxTopologyScopeSummaryDto>> {
    let pattern = format!("scope:%:{import_set_id}%");
    let mut statement = conn.prepare(
        "SELECT id, scope_kind, name, identity_state, status,
                evidence_completeness, diagnostics_json
         FROM linux_topology_scopes
         WHERE case_id = ?1 AND id LIKE ?2 ORDER BY id",
    )?;
    let rows = statement.query_map(params![case_id.0, pattern], |row| {
        Ok(ScopeRow {
            id: row.get(0)?,
            kind: row.get(1)?,
            name: row.get(2)?,
            identity_state: row.get(3)?,
            status: row.get(4)?,
            completeness: row.get(5)?,
            diagnostics: parse_diagnostics(&row.get::<_, String>(6)?),
        })
    })?;
    let mut scopes = Vec::new();
    for row in rows {
        let row = row?;
        let member_roles = load_scope_members(conn, &row.id)?;
        let member_source_ids = member_roles
            .iter()
            .map(|member| member.data_source_id.clone())
            .collect::<Vec<_>>();
        scopes.push(LinuxTopologyScopeSummaryDto {
            id: row.id,
            kind: row.kind,
            name: row.name,
            identity_state: row.identity_state,
            status: row.status,
            evidence_completeness: row.completeness,
            member_count: member_source_ids.len() as u32,
            member_source_ids,
            member_roles,
            diagnostics: row.diagnostics,
        });
    }
    Ok(scopes)
}

fn load_scope_members(
    conn: &Connection,
    scope_id: &str,
) -> Result<Vec<LinuxTopologyMemberSummaryDto>> {
    let mut statement = conn.prepare(
        "SELECT data_source_id, role, member_index, confidence
         FROM linux_topology_memberships
         WHERE scope_id = ?1 ORDER BY member_index, data_source_id",
    )?;
    let rows = statement.query_map([scope_id], |row| {
        Ok(LinuxTopologyMemberSummaryDto {
            data_source_id: row.get(0)?,
            role: row.get(1)?,
            member_index: row
                .get::<_, Option<i64>>(2)?
                .and_then(|value| u32::try_from(value).ok()),
            confidence: row.get(3)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn load_edges(
    conn: &Connection,
    scope_ids: &BTreeSet<String>,
) -> Result<Vec<LinuxTopologyEdgeSummaryDto>> {
    let mut statement = conn.prepare(
        "SELECT source_scope_id, target_scope_id, edge_kind, confidence, provenance_json
         FROM linux_topology_edges ORDER BY source_scope_id, target_scope_id, edge_kind",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(LinuxTopologyEdgeSummaryDto {
            source_scope_id: row.get(0)?,
            target_scope_id: row.get(1)?,
            kind: row.get(2)?,
            confidence: row.get(3)?,
            provenance: row.get(4)?,
        })
    })?;
    rows.filter_map(|row| match row {
        Ok(edge)
            if scope_ids.contains(&edge.source_scope_id)
                && scope_ids.contains(&edge.target_scope_id) =>
        {
            Some(Ok(edge))
        }
        Ok(_) => None,
        Err(error) => Some(Err(error)),
    })
    .collect::<std::result::Result<Vec<_>, _>>()
    .map_err(Into::into)
}

fn load_derived_sources(
    conn: &Connection,
    sources: &BTreeMap<String, domain::DataSource>,
) -> Vec<LinuxDerivedSourceSummaryDto> {
    let repo = DataSourceRepo::new(conn);
    sources
        .values()
        .filter(|source| {
            matches!(
                source.kind,
                DataSourceKind::CephRbd | DataSourceKind::CephFs
            )
        })
        .map(|source| LinuxDerivedSourceSummaryDto {
            data_source_id: source.id.0.clone(),
            kind: source.kind.to_string(),
            import_state: repo
                .find_storage(&source.id)
                .ok()
                .flatten()
                .map(|storage| storage.import_state)
                .unwrap_or_else(|| "unknown".to_string()),
            provenance_status: format!("{:?}", source.provenance.provenance_status)
                .to_ascii_lowercase(),
            source_path: source.source_path.display().to_string(),
        })
        .collect()
}

fn capability_level(
    import_set: &ImportSetRow,
    derived_sources: &[LinuxDerivedSourceSummaryDto],
) -> String {
    if derived_sources
        .iter()
        .any(|source| source.import_state == "ready")
    {
        "bounded_preview".to_string()
    } else if import_set.ready_count > 0 {
        "metadata_only".to_string()
    } else {
        "unsupported".to_string()
    }
}

fn parse_diagnostics(value: &str) -> Vec<String> {
    serde_json::from_str(value).unwrap_or_else(|_| vec![value.to_string()])
}
