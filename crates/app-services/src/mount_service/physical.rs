use std::path::PathBuf;

use domain::{DataSourceId, DataSourceKind};
use persistence_sqlite::repositories::{
    audit_repo::{AuditAction, AuditRepo},
    datasource_repo::DataSourceRepo,
};
use rusqlite::Connection;
use transport::dto::MountStatusDto;

use super::source_validation::{
    mount_source_binding, validate_source_identity, validate_source_kind,
};
use super::MountServiceError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreparedPhysicalImageKind {
    E01,
    Raw,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedPhysicalMountSource {
    pub source_path: PathBuf,
    pub image_kind: PreparedPhysicalImageKind,
    pub source_binding: String,
}

pub fn prepare_physical_mount_source(
    case_conn: &Connection,
    data_source_id: &DataSourceId,
) -> Result<PreparedPhysicalMountSource, MountServiceError> {
    let repo = DataSourceRepo::new(case_conn);
    let storage = repo
        .find_storage(data_source_id)?
        .ok_or_else(|| MountServiceError::NotFound("data source".to_string()))?;
    if !storage.import_state.eq_ignore_ascii_case("ready") {
        return Err(MountServiceError::SourceNotReady(storage.import_state));
    }
    let source_path = PathBuf::from(repo.source_path(data_source_id)?);
    let source_kind = repo.source_kind(data_source_id)?;
    validate_source_kind(&source_kind)?;
    validate_source_identity(&source_path, repo.source_evidence_size(data_source_id)?)?;
    Ok(PreparedPhysicalMountSource {
        source_path,
        image_kind: prepared_kind(&source_kind)?,
        source_binding: mount_source_binding(
            data_source_id,
            repo.source_fingerprint(data_source_id)?,
        ),
    })
}

pub fn record_physical_mount_audit(
    case_conn: &Connection,
    case_id: &domain::CaseId,
    status: &MountStatusDto,
) -> Result<(), MountServiceError> {
    let details = serde_json::json!({
        "status": "mounted",
        "mode": "physicalDisk",
        "mountId": status.target.mount_id,
        "physicalDevicePath": status.target.physical_device_path,
        "targetAddress": status.target.target_address,
        "readOnly": true,
    });
    AuditRepo::new(case_conn).log(
        Some(&case_id.0),
        "system",
        &AuditAction::ImageMount,
        Some(&status.target.data_source_id),
        &details.to_string(),
    )?;
    Ok(())
}

fn prepared_kind(
    source_kind: &DataSourceKind,
) -> Result<PreparedPhysicalImageKind, MountServiceError> {
    match source_kind {
        DataSourceKind::E01 => Ok(PreparedPhysicalImageKind::E01),
        DataSourceKind::Raw => Ok(PreparedPhysicalImageKind::Raw),
        other => Err(MountServiceError::Unsupported(format!(
            "data source kind '{other}' cannot be presented as a physical disk"
        ))),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/mount_service/physical.rs"]
mod tests;
