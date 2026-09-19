use crate::ledger_service::{get_proof, get_snapshot, seal_next_batch};
use domain::{verify_merkle_proof, MerkleProof};
use persistence_sqlite::connection::open_in_memory;
use persistence_sqlite::migrations::runner;
use persistence_sqlite::repositories::audit_repo::{AuditAction, AuditRepo};
use transport::dto::{GetLedgerProofRequest, GetLedgerRequest};

fn setup_conn() -> rusqlite::Connection {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    conn
}

#[test]
fn snapshot_seal_and_proof_round_trip() {
    let conn = setup_conn();
    let audit = AuditRepo::new(&conn);
    audit
        .log_simple(Some("case-1"), &AuditAction::CaseCreate, Some("case-1"))
        .unwrap();
    audit
        .log_simple(Some("case-1"), &AuditAction::CaseOpen, Some("case-1"))
        .unwrap();

    let before = get_snapshot(
        &conn,
        "case-1",
        GetLedgerRequest {
            limit: None,
            offset: None,
        },
    )
    .unwrap();
    assert_eq!(before.entries.len(), 2);
    assert!(before.verification.valid);
    assert!(before.batches.is_empty());

    let batch = seal_next_batch(&conn, "case-1").unwrap().unwrap();
    assert_eq!(batch.entry_count, 2);
    let proof = get_proof(
        &conn,
        "case-1",
        GetLedgerProofRequest {
            batch_id: batch.id.clone(),
            sequence: 2,
        },
    )
    .unwrap();
    let internal = MerkleProof {
        leaf_hash: proof.entry_hash.clone(),
        leaf_index: proof.leaf_index,
        leaf_count: proof.leaf_count,
        steps: proof
            .steps
            .iter()
            .map(|step| domain::MerkleProofStep {
                sibling_hash: step.sibling_hash.clone(),
                sibling_on_left: step.sibling_on_left,
            })
            .collect(),
    };
    assert!(verify_merkle_proof(&internal, &proof.merkle_root));

    let after = get_snapshot(
        &conn,
        "case-1",
        GetLedgerRequest {
            limit: None,
            offset: None,
        },
    )
    .unwrap();
    assert!(after.verification.valid);
    assert_eq!(after.batches.len(), 1);
}

#[test]
fn sealing_is_incremental_and_case_scoped() {
    let conn = setup_conn();
    let audit = AuditRepo::new(&conn);
    audit
        .log_simple(Some("case-1"), &AuditAction::CaseCreate, Some("case-1"))
        .unwrap();
    audit
        .log_simple(Some("case-2"), &AuditAction::CaseCreate, Some("case-2"))
        .unwrap();

    let first = seal_next_batch(&conn, "case-1").unwrap().unwrap();
    assert!(seal_next_batch(&conn, "case-1").unwrap().is_none());
    let second = seal_next_batch(&conn, "case-2").unwrap().unwrap();
    assert_ne!(first.merkle_root, second.merkle_root);
}

#[test]
fn snapshot_rejects_unbounded_page_requests() {
    let conn = setup_conn();
    let zero = get_snapshot(
        &conn,
        "case-1",
        GetLedgerRequest {
            limit: Some(0),
            offset: None,
        },
    )
    .unwrap_err();
    assert!(zero.to_string().contains("page size"));

    let oversized = get_snapshot(
        &conn,
        "case-1",
        GetLedgerRequest {
            limit: Some(501),
            offset: None,
        },
    )
    .unwrap_err();
    assert!(oversized.to_string().contains("page size"));
}

#[test]
fn proof_lookup_is_case_scoped() {
    let conn = setup_conn();
    AuditRepo::new(&conn)
        .log_simple(Some("case-1"), &AuditAction::CaseCreate, Some("case-1"))
        .unwrap();
    let batch = seal_next_batch(&conn, "case-1").unwrap().unwrap();

    let error = get_proof(
        &conn,
        "case-2",
        GetLedgerProofRequest {
            batch_id: batch.id,
            sequence: 1,
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("not found"));
}
