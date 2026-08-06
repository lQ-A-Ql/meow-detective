use std::path::PathBuf;

use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use transport::dto::{EmulationBootRouteDto, EmulationInstallDto, EmulationPreflightDto};

use super::{prepare_physical_mount_source, MountServiceError, PreparedPhysicalImageKind};
use crate::source_db::{self, ReadySourceError};

/// Upper bound for the partition scan; real images have a handful of
/// partitions and the loop stops at the first index without a root entry.
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
        if repo
            .find_root_for_partition(data_source_id, partition_index)?
            .is_none()
        {
            break;
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
        .find_by_partition_and_path(data_source_id, partition_index, path)?
        .is_some())
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
