use std::path::PathBuf;

use domain::{CaseId, DataSourceId};
use evidence_core::FileSystemReader;
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, file_repo::FileRepo, partition_repo::PartitionRepo,
};
use rusqlite::Connection;
use transport::dto::{EmulationBootRouteDto, EmulationInstallDto, EmulationPreflightDto};

use super::{prepare_physical_mount_source, MountServiceError, PreparedPhysicalImageKind};
use crate::source_db::{self, ReadySourceError};

/// Upper bound for the partition scan; real images have a handful of
/// partitions and indices may be sparse when system partitions are skipped.
const MAX_PREFLIGHT_PARTITIONS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedEmulationSource {
    pub source_path: PathBuf,
    pub image_kind: PreparedPhysicalImageKind,
    pub parent_sha256: [u8; 32],
}

pub fn prepare_emulation_source(
    case_conn: &Connection,
    data_source_id: &DataSourceId,
) -> Result<PreparedEmulationSource, MountServiceError> {
    let prepared = prepare_physical_mount_source(case_conn, data_source_id)?;
    let parent_sha256 = parse_source_sha256(&prepared.source_binding)?;
    Ok(PreparedEmulationSource {
        source_path: prepared.source_path,
        image_kind: prepared.image_kind,
        parent_sha256,
    })
}

/// Read-only pre-flight against the import-built file catalog: locates the
/// Windows installations, their OSDATA/SAM hives, and utilman bypass
/// feasibility without touching the evidence image itself.
pub fn emulation_preflight(
    case_conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<EmulationPreflightDto, MountServiceError> {
    prepare_physical_mount_source(case_conn, data_source_id)?;
    let source =
        source_db::open_ready_source_read_only_by_id(case_conn, case_root, case_id, data_source_id)
            .map_err(preflight_source_error)?;
    let repo = FileRepo::new(&source.connection);
    let mut installs = Vec::new();
    for partition_index in 0..MAX_PREFLIGHT_PARTITIONS {
        // Partition indices are not guaranteed dense: an import may skip
        // system partitions, so a missing root must not end the scan.
        if repo
            .find_root_for_partition(data_source_id, partition_index)?
            .is_none()
        {
            continue;
        }
        if !path_present(
            &repo,
            data_source_id,
            partition_index,
            "Windows/System32/config/SYSTEM",
        )? {
            continue;
        }
        let utilman = path_present(
            &repo,
            data_source_id,
            partition_index,
            "Windows/System32/utilman.exe",
        )?;
        let cmd = path_present(
            &repo,
            data_source_id,
            partition_index,
            "Windows/System32/cmd.exe",
        )?;
        installs.push(EmulationInstallDto {
            partition_index: partition_index as u32,
            osdata_present: path_present(
                &repo,
                data_source_id,
                partition_index,
                "Windows/System32/config/OSDATA",
            )?,
            sam_present: path_present(
                &repo,
                data_source_id,
                partition_index,
                "Windows/System32/config/SAM",
            )?,
            utilman_bypass_available: utilman && cmd,
        });
    }
    let recommended_boot_route = if installs.iter().any(|install| install.osdata_present) {
        EmulationBootRouteDto::RecoveryMedia
    } else {
        EmulationBootRouteDto::DirectSystem
    };
    enrich_bypass_from_fs(case_conn, data_source_id, &source, &mut installs);
    Ok(EmulationPreflightDto {
        data_source_id: data_source_id.0.clone(),
        installs,
        recommended_boot_route,
        maintenance_tool_available: false,
    })
}

fn path_present(
    repo: &FileRepo<'_>,
    data_source_id: &DataSourceId,
    partition_index: usize,
    path: &str,
) -> Result<bool, MountServiceError> {
    Ok(repo
        .find_by_partition_and_path_ci(data_source_id, partition_index, path)?
        .is_some())
}

/// The file catalog is inode-keyed: hard-linked system binaries such as
/// `cmd.exe` and `Utilman.exe` appear only under their WinSxS payload name,
/// so a catalog miss must fall back to the NTFS directory index (ground
/// truth) before declaring the Utilman bypass unavailable. Failures here are
/// non-fatal — the catalog verdict stands.
fn enrich_bypass_from_fs(
    case_conn: &Connection,
    data_source_id: &DataSourceId,
    source: &crate::source_db::ReadySourceConnection,
    installs: &mut [EmulationInstallDto],
) {
    let missing: Vec<u32> = installs
        .iter()
        .filter(|install| !install.utilman_bypass_available)
        .map(|install| install.partition_index)
        .collect();
    if missing.is_empty() {
        return;
    }
    let repo = DataSourceRepo::new(case_conn);
    let (source_path, source_kind) = match repo
        .source_path(data_source_id)
        .and_then(|path| repo.source_kind(data_source_id).map(|kind| (path, kind)))
    {
        Ok(parts) => parts,
        Err(error) => {
            tracing::warn!(error = %error, "bypass fs fallback: source metadata unavailable");
            return;
        }
    };
    let partitions = PartitionRepo::new(&source.connection);
    for partition_index in missing {
        match probe_bypass_binaries(
            &repo_context(&source_path, &source_kind),
            &partitions,
            data_source_id,
            partition_index,
        ) {
            Some(true) => {
                if let Some(install) = installs
                    .iter_mut()
                    .find(|install| install.partition_index == partition_index)
                {
                    install.utilman_bypass_available = true;
                }
            }
            Some(false) => {}
            None => {
                tracing::warn!(
                    partition_index,
                    "bypass fs fallback: partition could not be probed"
                );
            }
        }
    }
}

struct EvidenceContext {
    source_path: std::path::PathBuf,
    kind: domain::DataSourceKind,
}

fn repo_context(
    source_path: &str,
    source_kind: &domain::DataSourceKind,
) -> EvidenceContext {
    EvidenceContext {
        source_path: PathBuf::from(source_path),
        kind: source_kind.clone(),
    }
}

/// Returns `Some(true/false)` when the filesystem could be listed, `None`
/// when the partition is unavailable for a filesystem-level probe.
fn probe_bypass_binaries(
    context: &EvidenceContext,
    partitions: &PartitionRepo<'_>,
    data_source_id: &DataSourceId,
    partition_index: u32,
) -> Option<bool> {
    let record = partitions
        .find_by_data_source_and_index(&data_source_id.0, partition_index as usize)
        .ok()??;
    let reader =
        crate::datasource_service::open_evidence_reader(&context.source_path, &context.kind)
            .ok()?;
    let length = (record.length > 0).then_some(record.length);
    let window = evidence_core::PartitionWindowReader::new(reader, record.offset, length).ok()?;
    let fs = fs_ntfs::NtfsReader::open(Box::new(window), 0).ok()?;
    let children = fs.list_children("Windows/System32").ok()?;
    let present = |name: &str| {
        children
            .iter()
            .any(|node| !node.is_dir && node.name.eq_ignore_ascii_case(name))
    };
    Some(present("utilman.exe") && present("cmd.exe"))
}

fn preflight_source_error(error: ReadySourceError) -> MountServiceError {
    match error {
        ReadySourceError::Db(error) => MountServiceError::Database(error),
        ReadySourceError::NotFound { .. } => MountServiceError::NotFound("data source".to_string()),
        other => MountServiceError::SourceNotReady(other.to_string()),
    }
}

fn parse_source_sha256(value: &str) -> Result<[u8; 32], MountServiceError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(MountServiceError::InvalidSourceFingerprint);
    }
    let decoded = hex::decode(value).map_err(|_| MountServiceError::InvalidSourceFingerprint)?;
    decoded
        .try_into()
        .map_err(|_| MountServiceError::InvalidSourceFingerprint)
}

#[cfg(test)]
#[path = "../../tests/unit/mount_service/emulation.rs"]
mod tests;
