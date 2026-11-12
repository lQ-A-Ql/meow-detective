use super::*;

use domain::CaseId;
use persistence_sqlite::repositories::audit_repo::AuditRepo;
use transport::dto::{MountModeDto, MountStateDto, MountStatusDto, MountTargetDto};

fn logical_mount_status() -> MountStatusDto {
    MountStatusDto {
        target: MountTargetDto {
            mount_id: "mount-1".to_string(),
            data_source_id: "source-logical".to_string(),
            partition_index: 2,
            filesystem: "ntfs".to_string(),
            mount_point: "M:".to_string(),
            read_only: true,
            mode: MountModeDto::LogicalPartition,
            physical_device_path: None,
            target_address: None,
        },
        state: MountStateDto::Mounted,
        active_handle_count: 0,
        error: None,
    }
}

#[test]
fn logical_mount_audit_preserves_mount_identity_and_target() {
    let connection = persistence_sqlite::open_in_memory().expect("open audit database");
    persistence_sqlite::runner::run_all(&connection).expect("run audit schema migrations");

    record_logical_mount_audit(
        &connection,
        &CaseId("case-mount".to_string()),
        &logical_mount_status(),
    )
    .expect("persist logical mount audit");

    let entries = AuditRepo::new(&connection)
        .query(Some("case-mount"), Some("image.mount"), 10, 0)
        .expect("query logical mount audit");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].resource_id.as_deref(), Some("source-logical"));
    let details: serde_json::Value =
        serde_json::from_str(&entries[0].details).expect("parse audit details");
    assert_eq!(details["status"], "mounted");
    assert_eq!(details["mountId"], "mount-1");
    assert_eq!(details["partitionIndex"], 2);
    assert_eq!(details["filesystem"], "ntfs");
    assert_eq!(details["mountPoint"], "M:");
    assert_eq!(details["readOnly"], true);
}

#[test]
fn logical_mount_audit_reports_database_failure_for_command_rollback() {
    let connection = rusqlite::Connection::open_in_memory().expect("open unmigrated database");

    let error = record_logical_mount_audit(
        &connection,
        &CaseId("case-mount".to_string()),
        &logical_mount_status(),
    )
    .expect_err("missing audit schema must fail");

    assert!(matches!(error, MountServiceError::Database(_)));
}

#[test]
fn image_unmount_audit_records_requested_transition() {
    let connection = persistence_sqlite::open_in_memory().expect("open audit database");
    persistence_sqlite::runner::run_all(&connection).expect("run audit schema migrations");

    record_image_unmount_audit(
        &connection,
        &CaseId("case-mount".to_string()),
        &logical_mount_status(),
    )
    .expect("persist unmount audit");

    let entries = AuditRepo::new(&connection)
        .query(Some("case-mount"), Some("image.unmount"), 10, 0)
        .expect("query unmount audit");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].resource_id.as_deref(), Some("source-logical"));
    let details: serde_json::Value =
        serde_json::from_str(&entries[0].details).expect("parse audit details");
    assert_eq!(details["status"], "requested");
    assert_eq!(details["mountId"], "mount-1");
    assert_eq!(details["partitionIndex"], 2);
    assert_eq!(details["filesystem"], "ntfs");
    assert_eq!(details["mountPoint"], "M:");
    assert_eq!(details["readOnly"], true);
}
