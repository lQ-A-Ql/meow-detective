use super::*;

use evidence_core::LogicalFsReader;
use persistence_sqlite::repositories::{
    audit_repo::AuditRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    file_repo::FileRepo,
};
use tempfile::TempDir;

fn extraction() -> FileExtractionResultDto {
    FileExtractionResultDto {
        file_id: "ds:source:file".to_string(),
        bytes_written: 10,
        source_size: Some(10),
        sha256: "a".repeat(64),
        destination_file_name: "evidence.bin".to_string(),
        size_verified: true,
        audit_persisted: false,
        warning: None,
    }
}

#[test]
fn completed_extraction_surfaces_audit_failure_as_partial_success() {
    let result = resolve_extraction_with_audit(
        Ok(extraction()),
        Err(CommandError::internal("audit unavailable")),
    )
    .expect("the published file remains a successful extraction");

    assert!(!result.audit_persisted);
    assert!(result
        .warning
        .as_deref()
        .is_some_and(|warning| warning.contains("audit record")));
}

#[test]
fn completed_extraction_marks_audit_as_persisted_only_after_success() {
    let mut stale = extraction();
    stale.warning = Some("stale warning".to_string());

    let result = resolve_extraction_with_audit(Ok(stale), Ok(()))
        .expect("successful audit should preserve extraction success");

    assert!(result.audit_persisted);
    assert!(result.warning.is_none());
}

#[test]
fn outcome_audit_records_success_payload() {
    let connection = persistence_sqlite::open_in_memory().expect("open audit database");
    persistence_sqlite::runner::run_all(&connection).expect("run audit schema migrations");
    let outcome = Ok(extraction());

    audit_extraction_outcome(
        &connection,
        Some("case-extract"),
        "ds:source:file",
        Path::new("exports/evidence.bin"),
        true,
        &outcome,
    )
    .expect("persist success audit");

    let entries = AuditRepo::new(&connection)
        .query(Some("case-extract"), Some("file.extract"), 10, 0)
        .expect("query extraction audit");
    assert_eq!(entries.len(), 1);
    let details: serde_json::Value =
        serde_json::from_str(&entries[0].details).expect("parse audit details");
    assert_eq!(details["status"], "ok");
    assert_eq!(details["overwrite"], true);
    assert_eq!(details["destinationFileName"], "evidence.bin");
    assert_eq!(details["bytesWritten"], 10);
    assert_eq!(details["sourceSize"], 10);
    assert_eq!(details["sha256"], "a".repeat(64));
    assert_eq!(details["sizeVerified"], true);
}

#[test]
fn outcome_audit_records_failure_payload_with_command_error_identity() {
    let connection = persistence_sqlite::open_in_memory().expect("open audit database");
    persistence_sqlite::runner::run_all(&connection).expect("run audit schema migrations");
    let outcome: Result<FileExtractionResultDto, CommandError> =
        Err(CommandError::io("write failed"));

    audit_extraction_outcome(
        &connection,
        Some("case-extract"),
        "ds:source:file",
        Path::new("exports/evidence.bin"),
        false,
        &outcome,
    )
    .expect("persist failure audit");

    let entries = AuditRepo::new(&connection)
        .query(Some("case-extract"), Some("file.extract"), 10, 0)
        .expect("query extraction audit");
    assert_eq!(entries.len(), 1);
    let details: serde_json::Value =
        serde_json::from_str(&entries[0].details).expect("parse audit details");
    assert_eq!(details["status"], "failed");
    assert_eq!(details["overwrite"], false);
    assert_eq!(details["errorCode"], "IO_ERROR");
    assert_eq!(details["errorCategory"], "io");
}

#[test]
fn audited_extraction_writes_destination_and_persists_audit() {
    let temporary = TempDir::new().unwrap();
    let evidence_dir = temporary.path().join("evidence");
    std::fs::create_dir_all(&evidence_dir).unwrap();
    std::fs::write(evidence_dir.join("note.txt"), b"extract me").unwrap();

    let active = crate::case_service::create_case(
        &temporary.path().join("cases"),
        "audited-extract",
        Some("tester"),
    )
    .unwrap();

    active
        .with_conn(|connection| {
            let data_source_id = domain::DataSourceId("ds-extract".to_string());
            let data_source = domain::DataSource {
                id: data_source_id.clone(),
                name: "evidence".to_string(),
                kind: domain::DataSourceKind::LogicalDirectory,
                source_path: evidence_dir.clone(),
                imported_at: chrono::Utc::now(),
                provenance: domain::DataSourceProvenance::unknown(),
            };
            let mut storage =
                DataSourceStorage::source_db(&data_source_id.0, Some("windows"), None);
            storage.import_state = "ready".to_string();
            DataSourceRepo::new(connection).insert_with_storage(
                &active.meta.id,
                &data_source,
                &storage,
            )?;
            let source_connection =
                crate::source_db::open_source_db(&active.case_root, &data_source_id)?;
            DataSourceRepo::new(&source_connection)
                .upsert_source_local_metadata(&active.meta.id, &data_source)?;

            let filesystem = LogicalFsReader::open(&evidence_dir, "evidence")
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            crate::file_service::enumerate_filesystem(
                &source_connection,
                &data_source_id,
                &filesystem,
            )?;

            let local_file_id = FileRepo::new(&source_connection)
                .find_by_data_source(&data_source_id)?
                .into_iter()
                .find(|entry| entry.name == "note.txt")
                .map(|entry| entry.id.0)
                .expect("note.txt should be enumerated");
            let file_id = crate::source_db::GlobalFileId::new(
                data_source_id,
                domain::FileEntryId(local_file_id),
            )
            .encode()
            .0;
            let destination = temporary.path().join("exports").join("note-copy.txt");
            let bitlocker_runtime = Arc::new(BitLockerUnlockRegistry::default());
            let mut progress = |_update| {};

            let written = extract_file_for_case_with_audit(
                &bitlocker_runtime,
                CaseFileExtractionRequest {
                    case_conn: connection,
                    case_root: &active.case_root,
                    case_id: &active.meta.id,
                    file_id: &file_id,
                    destination_path: &destination,
                    overwrite: false,
                },
                &mut progress,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;

            assert_eq!(written.bytes_written, 10);
            assert!(written.audit_persisted);
            assert!(written.warning.is_none());
            assert_eq!(std::fs::read(&destination).unwrap(), b"extract me");

            let entries = AuditRepo::new(connection).query(
                Some(&active.meta.id.0),
                Some("file.extract"),
                10,
                0,
            )?;
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].resource_id.as_deref(), Some(file_id.as_str()));
            let details: serde_json::Value =
                serde_json::from_str(&entries[0].details).expect("parse audit details");
            assert_eq!(details["status"], "ok");
            Ok(())
        })
        .unwrap();
}
