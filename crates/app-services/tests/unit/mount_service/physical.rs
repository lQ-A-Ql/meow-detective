use domain::{CaseId, DataSourceKind};
use persistence_sqlite::repositories::audit_repo::AuditRepo;
use transport::dto::{MountModeDto, MountStateDto, MountStatusDto, MountTargetDto};

use super::{
    prepared_kind, record_physical_mount_audit, MountServiceError, PreparedPhysicalImageKind,
};

#[test]
fn physical_mount_kind_accepts_only_e01_and_raw_images() {
    assert_eq!(
        prepared_kind(&DataSourceKind::E01).unwrap(),
        PreparedPhysicalImageKind::E01
    );
    assert_eq!(
        prepared_kind(&DataSourceKind::Raw).unwrap(),
        PreparedPhysicalImageKind::Raw
    );
    assert!(prepared_kind(&DataSourceKind::LogicalDirectory).is_err());
}

#[test]
fn physical_mount_audit_preserves_mount_identity_and_read_only_target() {
    let connection = persistence_sqlite::open_in_memory().expect("open audit database");
    persistence_sqlite::runner::run_all(&connection).expect("run audit schema migrations");
    let status = physical_mount_status();

    record_physical_mount_audit(&connection, &CaseId("case-physical".to_string()), &status)
        .expect("persist physical mount audit");

    let entries = AuditRepo::new(&connection)
        .query(Some("case-physical"), Some("image.mount"), 10, 0)
        .expect("query physical mount audit");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].resource_id.as_deref(), Some("source-physical"));
    let details: serde_json::Value =
        serde_json::from_str(&entries[0].details).expect("parse audit details");
    assert_eq!(details["mountId"], "physical-1");
    assert_eq!(details["mode"], "physicalDisk");
    assert_eq!(details["readOnly"], true);
    assert_eq!(details["physicalDevicePath"], r"\\.\PhysicalDrive42");
    assert_eq!(details["targetAddress"], "127.0.0.1:3260");
}

#[test]
fn physical_mount_audit_reports_database_failure_for_command_rollback() {
    let connection = rusqlite::Connection::open_in_memory().expect("open unmigrated database");

    let error = record_physical_mount_audit(
        &connection,
        &CaseId("case-physical".to_string()),
        &physical_mount_status(),
    )
    .expect_err("missing audit schema must fail");

    assert!(matches!(error, MountServiceError::Database(_)));
}

fn physical_mount_status() -> MountStatusDto {
    MountStatusDto {
        target: MountTargetDto {
            mount_id: "physical-1".to_string(),
            data_source_id: "source-physical".to_string(),
            partition_index: 0,
            filesystem: "physical-disk".to_string(),
            mount_point: r"\\.\PhysicalDrive42".to_string(),
            read_only: true,
            mode: MountModeDto::PhysicalDisk,
            physical_device_path: Some(r"\\.\PhysicalDrive42".to_string()),
            target_address: Some("127.0.0.1:3260".to_string()),
        },
        state: MountStateDto::Mounted,
        active_handle_count: 0,
        error: None,
    }
}
