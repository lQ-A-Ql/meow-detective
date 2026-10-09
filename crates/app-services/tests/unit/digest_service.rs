use crate::digest_service::{calculate_evidence_digest, DigestServiceError, DigestTarget};
use domain::{CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
};
use rusqlite::Connection;
use std::path::Path;
use transport::dto::{DigestAlgorithmDto, DigestScopeDto};

fn case_with_source(kind: DataSourceKind, path: &str) -> (Connection, CaseId) {
    let connection = persistence_sqlite::open_in_memory().expect("connection");
    persistence_sqlite::runner::run_all(&connection).expect("migrations");
    let case_id = CaseId("digest-case".to_string());
    CaseRepo::new(&connection)
        .create(&CaseMeta {
            id: case_id.clone(),
            name: "Digest test".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .expect("case");
    let source = DataSource {
        id: DataSourceId("source-1".to_string()),
        name: "source-1".to_string(),
        kind,
        source_path: path.into(),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    DataSourceRepo::new(&connection)
        .insert_with_storage(
            &case_id,
            &source,
            &DataSourceStorage::source_db("source-1", Some("windows"), None),
        )
        .expect("source");
    (connection, case_id)
}

#[test]
fn unsupported_partition_scope_fails_closed() {
    let connection = Connection::open_in_memory().expect("connection");
    let result = calculate_evidence_digest(
        &connection,
        Path::new("."),
        &CaseId("case".into()),
        DigestScopeDto::Partition,
        DigestAlgorithmDto::Sha256,
        DigestTarget {
            data_source_id: Some("source"),
            file_id: None,
            partition_index: Some(0),
        },
    );
    assert!(matches!(result, Err(DigestServiceError::Unsupported)));
}

#[test]
fn logical_disk_scope_rejects_e01_container() {
    let (connection, case_id) = case_with_source(DataSourceKind::E01, "sample.E01");
    let result = calculate_evidence_digest(
        &connection,
        Path::new("."),
        &case_id,
        DigestScopeDto::LogicalDisk,
        DigestAlgorithmDto::Sha256,
        DigestTarget {
            data_source_id: Some("source-1"),
            file_id: None,
            partition_index: None,
        },
    );
    assert!(matches!(result, Err(DigestServiceError::Unsupported)));
}

#[test]
fn logical_disk_scope_uses_physical_reader_for_local_disk() {
    let (connection, case_id) =
        case_with_source(DataSourceKind::LocalDisk, r"\\.\PhysicalDrive9999");
    let result = calculate_evidence_digest(
        &connection,
        Path::new("."),
        &case_id,
        DigestScopeDto::LogicalDisk,
        DigestAlgorithmDto::Sha256,
        DigestTarget {
            data_source_id: Some("source-1"),
            file_id: None,
            partition_index: None,
        },
    );
    assert!(matches!(
        result,
        Err(DigestServiceError::Hash(
            crate::hash_service::EvidenceHashError::Io { .. }
        ))
    ));
}
