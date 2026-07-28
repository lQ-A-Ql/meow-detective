use app_services::{case_service, file_service};
use evidence_core::LogicalFsReader;
use persistence_sqlite::repositories::{
    audit_repo::AuditRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    file_repo::FileRepo,
};
use tempfile::TempDir;
use transport::{dto::FileExtractionResultDto, CommandError};

use super::support::with_raw_exfat_case_file;

#[test]
fn extract_file_uses_entry_reader_and_writes_destination() {
    let temporary = TempDir::new().unwrap();
    let evidence_dir = temporary.path().join("evidence");
    std::fs::create_dir_all(&evidence_dir).unwrap();
    std::fs::write(evidence_dir.join("note.txt"), b"extract me").unwrap();

    let active =
        case_service::create_case(&temporary.path().join("cases"), "extract", Some("tester"))
            .unwrap();
    let case_id = active.meta.id.clone();

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
                &case_id,
                &data_source,
                &storage,
            )?;
            let source_connection =
                app_services::source_db::open_source_db(&active.case_root, &data_source_id)?;
            DataSourceRepo::new(&source_connection)
                .upsert_source_local_metadata(&case_id, &data_source)?;

            let filesystem = LogicalFsReader::open(&evidence_dir, "evidence")
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            file_service::enumerate_filesystem(&source_connection, &data_source_id, &filesystem)?;

            let local_file_id = FileRepo::new(&source_connection)
                .find_by_data_source(&data_source_id)?
                .into_iter()
                .find(|entry| entry.name == "note.txt")
                .map(|entry| entry.id.0)
                .expect("note.txt should be enumerated");
            let file_id = app_services::source_db::GlobalFileId::new(
                data_source_id,
                domain::FileEntryId(local_file_id),
            )
            .encode()
            .0;
            let destination = temporary.path().join("exports").join("note-copy.txt");
            let written = file_service::extract_file_to_destination_for_case(
                connection,
                &active.case_root,
                &active.meta.id,
                &file_id,
                &destination,
                false,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;

            assert_eq!(written.bytes_written, 10);
            assert_eq!(written.source_size, Some(10));
            assert_eq!(
                written.sha256,
                "805b8560cbda878ebc1eae0e5fdac9c0ed9172bcba8a263541c2a5ebd1cc26ac"
            );
            assert!(written.size_verified);
            assert!(!written.audit_persisted);
            assert_eq!(std::fs::read(&destination).unwrap(), b"extract me");

            Ok(())
        })
        .unwrap();
}

#[test]
fn file_extraction_audit_is_persisted_with_case_and_resource() {
    let connection = persistence_sqlite::open_in_memory().expect("open audit database");
    persistence_sqlite::runner::run_all(&connection).expect("run audit schema migrations");
    super::super::support::persist_file_extract_audit(
        &connection,
        Some("case-extract"),
        "ds:source:file",
        serde_json::json!({"status": "ok", "bytesWritten": 10}),
    )
    .expect("persist extraction audit");

    let entries = AuditRepo::new(&connection)
        .query(Some("case-extract"), Some("file.extract"), 10, 0)
        .expect("query extraction audit");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].resource_id.as_deref(), Some("ds:source:file"));
    assert!(entries[0].details.contains("bytesWritten"));
}

#[test]
fn file_extraction_audit_failure_is_not_silently_ignored() {
    let connection = rusqlite::Connection::open_in_memory().expect("open unmigrated database");
    let error = super::super::support::persist_file_extract_audit(
        &connection,
        Some("case-extract"),
        "ds:source:file",
        serde_json::json!({"status": "ok"}),
    )
    .expect_err("missing audit schema must be reported");

    assert_eq!(error.category, "io");
}

#[test]
fn completed_extraction_surfaces_audit_failure_as_partial_success() {
    let extraction = FileExtractionResultDto {
        file_id: "ds:source:file".to_string(),
        bytes_written: 10,
        source_size: Some(10),
        sha256: "a".repeat(64),
        destination_file_name: "evidence.bin".to_string(),
        size_verified: true,
        audit_persisted: false,
        warning: None,
    };

    let result = super::super::extract::resolve_extract_and_audit(
        Ok(extraction),
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
    let extraction = FileExtractionResultDto {
        file_id: "ds:source:file".to_string(),
        bytes_written: 10,
        source_size: Some(10),
        sha256: "a".repeat(64),
        destination_file_name: "evidence.bin".to_string(),
        size_verified: true,
        audit_persisted: false,
        warning: Some("stale warning".to_string()),
    };

    let result = super::super::extract::resolve_extract_and_audit(Ok(extraction), Ok(()))
        .expect("successful audit should preserve extraction success");

    assert!(result.audit_persisted);
    assert!(result.warning.is_none());
}

#[test]
fn raw_source_extraction_uses_the_global_file_route() {
    with_raw_exfat_case_file(
        "raw-extraction",
        "bin",
        |connection, case_id, file_id, case_root| {
            let destination = case_root
                .parent()
                .expect("case workspace parent")
                .join("raw-export.bin");
            let extraction = file_service::extract_file_to_destination_for_case(
                connection,
                &case_root,
                &domain::CaseId(case_id),
                &file_id,
                &destination,
                false,
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;

            let exported = std::fs::read(&destination)?;
            assert_eq!(exported.len(), 1536);
            assert!(exported[..512].iter().all(|byte| *byte == b'A'));
            assert!(exported[512..1024].iter().all(|byte| *byte == b'B'));
            assert!(exported[1024..].iter().all(|byte| *byte == b'C'));
            assert_eq!(extraction.bytes_written, 1536);
            assert!(extraction.size_verified);
            Ok(())
        },
    );
}
