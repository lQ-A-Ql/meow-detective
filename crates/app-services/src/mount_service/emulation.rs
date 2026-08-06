use std::path::PathBuf;

use domain::DataSourceId;
use rusqlite::Connection;

use super::{prepare_physical_mount_source, MountServiceError, PreparedPhysicalImageKind};

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
