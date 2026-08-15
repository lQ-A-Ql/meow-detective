//! Audit records for logical image mount lifecycle transitions.

use persistence_sqlite::repositories::audit_repo::{AuditAction, AuditRepo};
use rusqlite::Connection;
use transport::dto::MountStatusDto;

use super::MountServiceError;

/// Persist the audit record for a freshly mounted logical partition.
pub fn record_logical_mount_audit(
    case_conn: &Connection,
    case_id: &domain::CaseId,
    status: &MountStatusDto,
) -> Result<(), MountServiceError> {
    let details = serde_json::json!({
        "status": "mounted",
        "mountId": status.target.mount_id,
        "partitionIndex": status.target.partition_index,
        "filesystem": status.target.filesystem,
        "mountPoint": status.target.mount_point,
        "readOnly": status.target.read_only,
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

/// Persist the audit record for a requested image unmount. Shared by logical
/// and physical mounts; both dispatch through the same target identity.
pub fn record_image_unmount_audit(
    case_conn: &Connection,
    case_id: &domain::CaseId,
    status: &MountStatusDto,
) -> Result<(), MountServiceError> {
    let details = serde_json::json!({
        "status": "requested",
        "mountId": status.target.mount_id,
        "partitionIndex": status.target.partition_index,
        "filesystem": status.target.filesystem,
        "mountPoint": status.target.mount_point,
        "readOnly": status.target.read_only,
    });
    AuditRepo::new(case_conn).log(
        Some(&case_id.0),
        "system",
        &AuditAction::ImageUnmount,
        Some(&status.target.data_source_id),
        &details.to_string(),
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/mount_service/audit.rs"]
mod tests;
