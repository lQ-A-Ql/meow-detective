use crate::digest_service::{calculate_evidence_digest, DigestServiceError, DigestTarget};
use domain::CaseId;
use rusqlite::Connection;
use std::path::Path;
use transport::dto::{DigestAlgorithmDto, DigestScopeDto};

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
