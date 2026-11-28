#[path = "support/registered_verification.rs"]
mod support;

use std::{
    io::{Read, Seek, SeekFrom},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

use evidence_access::structure_access::{
    open_verified_view, plan_registered_access, verify_registered_access, RegisteredAccessRequest,
};
use sha2::{Digest, Sha256};
use source_kernel::source_runtime::{
    lease::{invalidate_case, invalidate_source},
    read_scope::SourceReadScope,
    verification::{VerificationBatch, VerificationRequest},
    SourcePublicationLease,
};
use support::{attempt, bytes, demand, Fixture};

fn replace_raw_bytes(path: &std::path::Path) {
    use std::io::Write;
    let modified = path.metadata().unwrap().modified().unwrap();
    let mut file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    file.write_all(&[0xff; 64]).unwrap();
    file.sync_all().unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
}

#[test]
fn registered_raw_view_rejects_content_changes_with_unchanged_length_and_mtime() {
    let fixture = Fixture::new();
    let original = bytes();
    let source = fixture.source("view-integrity", &original, &[]);
    let cancel = AtomicBool::new(false);
    let plan = plan_registered_access(
        &fixture.conn,
        fixture.root.path(),
        RegisteredAccessRequest {
            demand: demand(&source, original.len() as u64, &[(0, 64)], "preview"),
            file_id: None,
        },
        &cancel,
    )
    .unwrap();
    let mut view = open_verified_view(plan, &cancel).unwrap();
    replace_raw_bytes(&fixture.path("view-integrity"));
    assert!(view.read_exact(&mut [0; 64]).is_err());
}

#[test]
fn registered_raw_receipt_rejects_changed_physical_bytes_in_the_same_owner_window() {
    let fixture = Fixture::new();
    let original = bytes();
    let source = fixture.source("receipt-integrity", &original, &[]);
    let batch = VerificationBatch::new(Some(attempt())).unwrap();
    let cancel = AtomicBool::new(false);
    let requested = demand(&source, original.len() as u64, &[(0, 64)], "preview");
    let expected = hex::encode(Sha256::digest(&original[..64]));
    let verify = || {
        verify_registered_access(
            &fixture.conn,
            fixture.root.path(),
            RegisteredAccessRequest {
                demand: requested.clone(),
                file_id: None,
            },
            &batch,
            "registered-source/1",
            Some(&expected),
            false,
            &cancel,
        )
    };
    let proof = verify().unwrap();
    replace_raw_bytes(&fixture.path("receipt-integrity"));
    assert!(verify().is_err());
    assert!(proof.validate().is_err());
}

#[test]
fn registered_ranges_match_source_bytes_and_receipt_covers_every_declared_byte() {
    let fixture = Fixture::new();
    let bytes = bytes();
    let source = fixture.source("source", &bytes, &[]);
    let demand = demand(
        &source,
        bytes.len() as u64,
        &[(3, 100), (1024, 4000)],
        "preview",
    );
    let oracle = [&bytes[3..103], &bytes[1024..5024]].concat();
    let batch = VerificationBatch::new(Some(attempt())).unwrap();
    let proof = verify_registered_access(
        &fixture.conn,
        fixture.root.path(),
        RegisteredAccessRequest {
            demand: demand.clone(),
            file_id: None,
        },
        &batch,
        "registered-source/1",
        Some(&hex::encode(Sha256::digest(&oracle))),
        false,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(proof.receipt().bytes_verified, 4100);
    assert_eq!(proof.receipt().ranges, demand.ranges);
    assert_eq!(proof.receipt().attempt, attempt());
    let plan = plan_registered_access(
        &fixture.conn,
        fixture.root.path(),
        RegisteredAccessRequest {
            demand,
            file_id: None,
        },
        &AtomicBool::new(false),
    )
    .unwrap();
    let cancel = AtomicBool::new(false);
    let mut reader = open_verified_view(plan, &cancel).unwrap();
    let mut output = Vec::new();
    reader.read_to_end(&mut output).unwrap();
    assert_eq!(output, oracle);
    reader.seek(SeekFrom::Start(100)).unwrap();
    let mut byte = [0];
    reader.read_exact(&mut byte).unwrap();
    assert_eq!(byte[0], bytes[1024]);
    let json = serde_json::to_string(proof.receipt()).unwrap();
    assert_eq!(
        serde_json::from_str::<domain::structure::VerificationReceipt>(&json).unwrap(),
        *proof.receipt()
    );
}

#[test]
fn range_digest_cannot_issue_a_native_checksum_receipt() {
    let fixture = Fixture::new();
    let bytes = bytes();
    let source = fixture.source("source", &bytes, &[]);
    let scope = SourceReadScope::acquire(fixture.root.path(), &source).unwrap();
    let batch = VerificationBatch::new(Some(attempt())).unwrap();
    let mut demand = demand(
        &source,
        bytes.len() as u64,
        &[(0, 16384)],
        "native-checksum",
    );
    demand.verification = domain::structure::ReadVerification::NativeChecksum;
    let opens = AtomicUsize::new(0);
    let result = batch.verify(
        scope,
        VerificationRequest {
            demand: &demand,
            adapter_version: "raw/1",
            policy_version: "bytes/1",
            expected_sha256: None,
            fresh: false,
        },
        &AtomicBool::new(false),
        || {
            opens.fetch_add(1, Ordering::Relaxed);
            Ok(support::raw(&fixture.path("source")))
        },
    );
    match result {
        Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::Unsupported),
        Ok(_) => panic!("native verification needs a native verifier"),
    }
    assert_eq!(opens.load(Ordering::Relaxed), 0);
    assert_eq!(batch.stats().unwrap().verified_bytes, 0);
}

#[test]
fn consumers_share_one_actual_verification_within_the_same_owner_window() {
    let fixture = Fixture::new();
    let bytes = bytes();
    let source = fixture.source("source", &bytes, &[]);
    let batch = VerificationBatch::new(Some(attempt())).unwrap();
    let opens = AtomicUsize::new(0);
    let mut proofs = Vec::new();
    for consumer in ["preview", "catalog"] {
        let demand = demand(&source, bytes.len() as u64, &[(0, 16384)], consumer);
        let scope = SourceReadScope::acquire(fixture.root.path(), &source).unwrap();
        proofs.push(
            batch
                .verify(
                    scope,
                    VerificationRequest {
                        demand: &demand,
                        adapter_version: "raw/1",
                        policy_version: "bytes/1",
                        expected_sha256: None,
                        fresh: false,
                    },
                    &AtomicBool::new(false),
                    || {
                        opens.fetch_add(1, Ordering::Relaxed);
                        Ok(support::raw(&fixture.path("source")))
                    },
                )
                .unwrap(),
        );
    }
    assert_eq!(opens.load(Ordering::Relaxed), 1);
    assert!(Arc::ptr_eq(&proofs[0], &proofs[1]));
    assert_eq!(batch.stats().unwrap().verified_bytes, 16384);
    assert_eq!(batch.stats().unwrap().reused_receipts, 1);
    assert_eq!(batch.stats().unwrap().source_scopes, 1);
}

#[test]
fn a_changed_parent_invalidates_its_descendants_and_preserves_an_unrelated_branch() {
    let fixture = Fixture::new();
    let bytes = bytes();
    let parent = fixture.source("parent", &bytes, &[]);
    let child = fixture.source(
        "derived-range",
        &bytes[0..4096],
        std::slice::from_ref(&parent),
    );
    let other = fixture.source("unrelated", &bytes, &[]);
    let child_scope = SourceReadScope::acquire(fixture.root.path(), &child).unwrap();
    let other_scope = SourceReadScope::acquire(fixture.root.path(), &other).unwrap();
    fixture.revise(&parent);
    assert!(child_scope.validate().is_err());
    other_scope.validate().unwrap();
}

#[test]
fn borrowed_reader_survives_owner_eviction_and_rejects_source_or_case_invalidation() {
    let fixture = Fixture::new();
    let bytes = bytes();
    let first = fixture.source("first", &bytes, &[]);
    let second = fixture.source("second", &bytes, &[]);
    let scope = SourceReadScope::acquire(fixture.root.path(), &first).unwrap();
    let mut reader = scope.wrap(support::raw(&fixture.path("first"))).unwrap();
    drop(scope);
    let mut output = [0; 16];
    reader.read_exact(&mut output).unwrap();
    assert_eq!(output, bytes[..16]);
    invalidate_source(&fixture.case_id.0, "first");
    assert!(reader.read(&mut output).is_err());
    let scope = SourceReadScope::acquire(fixture.root.path(), &second).unwrap();
    scope.validate().unwrap();
    invalidate_case(&fixture.case_id.0);
    assert!(scope.validate().is_err());
}

#[test]
fn generation_replacement_rejects_old_readers_and_does_not_revoke_other_sources() {
    let fixture = Fixture::new();
    let bytes = bytes();
    let first = fixture.source("first", &bytes, &[]);
    let second = fixture.source("second", &bytes, &[]);
    let old = SourceReadScope::acquire(fixture.root.path(), &first).unwrap();
    let other = SourceReadScope::acquire(fixture.root.path(), &second).unwrap();
    let control = source_kernel::source_db::open_source_db(
        fixture.root.path(),
        &domain::DataSourceId("first".into()),
    )
    .unwrap();
    support::seal(
        &control,
        "first",
        "replacement",
        &hex::encode(Sha256::digest(b"new-generation")),
    );
    assert!(old.validate().is_err());
    other.validate().unwrap();
}

#[test]
fn explicit_deep_verification_reads_again_and_invalidates_a_failed_branch() {
    let fixture = Fixture::new();
    let bytes = bytes();
    let source = fixture.source("source", &bytes, &[]);
    let unrelated = fixture.source("unrelated", &bytes, &[]);
    let other = SourceReadScope::acquire(fixture.root.path(), &unrelated).unwrap();
    let batch = VerificationBatch::new(Some(attempt())).unwrap();
    let demand = demand(
        &source,
        bytes.len() as u64,
        &[(0, 16384)],
        "deep-verification",
    );
    let expected = hex::encode(Sha256::digest(&bytes));
    let proof = verify_registered_access(
        &fixture.conn,
        fixture.root.path(),
        RegisteredAccessRequest {
            demand: demand.clone(),
            file_id: None,
        },
        &batch,
        "raw/1",
        Some(&expected),
        false,
        &AtomicBool::new(false),
    )
    .unwrap();
    let path = fixture.path("source");
    let original_modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::fs::write(&path, vec![42; bytes.len()]).unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(original_modified))
        .unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        original_modified
    );
    assert!(verify_registered_access(
        &fixture.conn,
        fixture.root.path(),
        RegisteredAccessRequest {
            demand,
            file_id: None
        },
        &batch,
        "raw/1",
        Some(&expected),
        true,
        &AtomicBool::new(false)
    )
    .is_err());
    assert!(proof.validate().is_err());
    other.validate().unwrap();
}

#[test]
fn cached_receipt_cannot_be_committed_by_a_superseded_publication_owner() {
    let fixture = Fixture::new();
    let bytes = bytes();
    let source = fixture.source("source", &bytes, &[]);
    let batch = VerificationBatch::new(Some(attempt())).unwrap();
    let proof = verify_registered_access(
        &fixture.conn,
        fixture.root.path(),
        RegisteredAccessRequest {
            demand: demand(&source, bytes.len() as u64, &[(0, 16384)], "publisher"),
            file_id: None,
        },
        &batch,
        "raw/1",
        None,
        false,
        &AtomicBool::new(false),
    )
    .unwrap();
    let factory = evidence_access::source_runtime::open_factory(
        &fixture.path("source"),
        &domain::DataSourceKind::Raw,
    )
    .unwrap();
    let old = SourcePublicationLease::acquire(&fixture.case_id.0, "source", factory.clone())
        .with_receipt(proof)
        .unwrap();
    let newer = SourcePublicationLease::acquire(&fixture.case_id.0, "source", factory);
    let output = rusqlite::Connection::open_in_memory().unwrap();
    output
        .execute_batch("CREATE TABLE published(value INTEGER)")
        .unwrap();
    let _fence = old.fence_connection(&output);
    assert!(output
        .execute("INSERT INTO published VALUES(1)", [])
        .is_err());
    drop(newer);
    assert!(old.validate().is_err());
    assert_eq!(
        output
            .query_row("SELECT COUNT(*) FROM published", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn cancellation_and_deadline_expiry_cannot_return_a_cached_success() {
    let fixture = Fixture::new();
    let bytes = bytes();
    let source = fixture.source("source", &bytes, &[]);
    let batch = VerificationBatch::new(Some(attempt())).unwrap();
    let demand = demand(&source, bytes.len() as u64, &[(0, 16384)], "reader");
    verify_registered_access(
        &fixture.conn,
        fixture.root.path(),
        RegisteredAccessRequest {
            demand: demand.clone(),
            file_id: None,
        },
        &batch,
        "raw/1",
        None,
        false,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(verify_registered_access(
        &fixture.conn,
        fixture.root.path(),
        RegisteredAccessRequest {
            demand: demand.clone(),
            file_id: None
        },
        &batch,
        "raw/1",
        None,
        false,
        &AtomicBool::new(true)
    )
    .is_err());
    let mut expired = demand;
    expired.deadline_unix_ms = Some(1);
    assert!(verify_registered_access(
        &fixture.conn,
        fixture.root.path(),
        RegisteredAccessRequest {
            demand: expired,
            file_id: None
        },
        &batch,
        "raw/1",
        None,
        false,
        &AtomicBool::new(false)
    )
    .is_err());
}

#[test]
fn simultaneous_consumers_reuse_one_range_receipt_and_policy_changes_require_new_reads() {
    let fixture = Fixture::new();
    let bytes = bytes();
    let source = fixture.source("source", &bytes, &[]);
    let scope = SourceReadScope::acquire(fixture.root.path(), &source).unwrap();
    let batch = Arc::new(VerificationBatch::new(Some(attempt())).unwrap());
    let opens = Arc::new(AtomicUsize::new(0));
    let start = Arc::new(std::sync::Barrier::new(3));
    let mut tasks = Vec::new();
    for consumer in ["preview", "catalog"] {
        let scope = scope.clone();
        let batch = batch.clone();
        let opens = opens.clone();
        let start = start.clone();
        let path = fixture.path("source");
        let demand = demand(&source, bytes.len() as u64, &[(0, 16384)], consumer);
        tasks.push(std::thread::spawn(move || {
            start.wait();
            batch
                .verify(
                    scope,
                    VerificationRequest {
                        demand: &demand,
                        adapter_version: "raw/1",
                        policy_version: "range/1",
                        expected_sha256: None,
                        fresh: false,
                    },
                    &AtomicBool::new(false),
                    || {
                        opens.fetch_add(1, Ordering::AcqRel);
                        Ok(support::raw(&path))
                    },
                )
                .unwrap()
        }));
    }
    start.wait();
    let receipts = tasks
        .into_iter()
        .map(|task| task.join().unwrap())
        .collect::<Vec<_>>();
    assert!(Arc::ptr_eq(&receipts[0], &receipts[1]));
    let demand = demand(&source, bytes.len() as u64, &[(0, 16384)], "preview");
    batch
        .verify(
            scope,
            VerificationRequest {
                demand: &demand,
                adapter_version: "raw/2",
                policy_version: "range/1",
                expected_sha256: None,
                fresh: false,
            },
            &AtomicBool::new(false),
            || {
                opens.fetch_add(1, Ordering::AcqRel);
                Ok(support::raw(&fixture.path("source")))
            },
        )
        .unwrap();
    assert_eq!(opens.load(Ordering::Acquire), 2);
}

#[test]
fn concurrent_generation_replacement_is_visible_to_borrowed_readers() {
    let fixture = Fixture::new();
    let source = fixture.source("source", &bytes(), &[]);
    let scope = SourceReadScope::acquire(fixture.root.path(), &source).unwrap();
    let mut reader = scope.wrap(support::raw(&fixture.path("source"))).unwrap();
    let ready = Arc::new(std::sync::Barrier::new(2));
    let replaced = Arc::new(std::sync::Barrier::new(2));
    let reader_ready = ready.clone();
    let reader_replaced = replaced.clone();
    let task = std::thread::spawn(move || {
        reader.read_exact(&mut [0; 16]).unwrap();
        reader_ready.wait();
        reader_replaced.wait();
        assert!(reader.read(&mut [0; 16]).is_err());
    });
    ready.wait();
    let control = source_kernel::source_db::open_source_db(
        fixture.root.path(),
        &domain::DataSourceId(source.source_id),
    )
    .unwrap();
    support::seal(
        &control,
        "source",
        "replacement",
        &hex::encode(Sha256::digest(b"replacement")),
    );
    replaced.wait();
    task.join().unwrap();
}

#[test]
fn parent_revision_change_revokes_attached_receipts_before_publication_commit() {
    let fixture = Fixture::new();
    let bytes = bytes();
    let parent = fixture.source("parent", &bytes, &[]);
    let source = fixture.source("child", &bytes, std::slice::from_ref(&parent));
    let batch = VerificationBatch::new(Some(attempt())).unwrap();
    let proof = verify_registered_access(
        &fixture.conn,
        fixture.root.path(),
        RegisteredAccessRequest {
            demand: demand(&source, bytes.len() as u64, &[(0, 16384)], "publisher"),
            file_id: None,
        },
        &batch,
        "raw/1",
        None,
        false,
        &AtomicBool::new(false),
    )
    .unwrap();
    let factory = evidence_access::source_runtime::open_factory(
        &fixture.path("child"),
        &domain::DataSourceKind::Raw,
    )
    .unwrap();
    let lease = SourcePublicationLease::acquire(&fixture.case_id.0, "child", factory)
        .with_receipt(proof)
        .unwrap();
    let output = rusqlite::Connection::open_in_memory().unwrap();
    output
        .execute_batch("CREATE TABLE published(value INTEGER)")
        .unwrap();
    let _fence = lease.fence_connection(&output);
    output
        .execute_batch("BEGIN; INSERT INTO published VALUES(1)")
        .unwrap();
    fixture.revise(&parent);
    assert!(output.execute_batch("COMMIT").is_err());
    assert_eq!(
        output
            .query_row("SELECT COUNT(*) FROM published", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn verified_physical_inputs_remain_readable_without_a_filesystem_catalog() {
    let fixture = Fixture::new();
    let bytes = bytes();
    let source = fixture.source("physical", &bytes, &[]);
    persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&fixture.conn)
        .update_import_state(&domain::DataSourceId("physical".into()), "pending", None)
        .unwrap();
    let batch = VerificationBatch::new(Some(attempt())).unwrap();
    let proof = verify_registered_access(
        &fixture.conn,
        fixture.root.path(),
        RegisteredAccessRequest {
            demand: demand(
                &source,
                bytes.len() as u64,
                &[(0, 16384)],
                "physical-inspection",
            ),
            file_id: None,
        },
        &batch,
        "raw/1",
        Some(&hex::encode(Sha256::digest(&bytes))),
        true,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(proof.receipt().bytes_verified, bytes.len() as u64);
    assert_eq!(
        proof.receipt().generation_identity,
        "verified-physical-revision:1"
    );
}

#[test]
fn borrowed_e01_reader_survives_metadata_cache_eviction_and_releases_its_owner_later() {
    let fixture = Fixture::new();
    let (source, bytes, path) = fixture.e01_source("e01");
    let factory = evidence_access::source_runtime::factory::open_factory_for_case(
        &path,
        &domain::DataSourceKind::E01,
        &AtomicBool::new(false),
        &fixture.case_id.0,
    )
    .unwrap();
    let runtime = evidence_access::source_runtime::SourceRuntime::new(
        evidence_access::source_runtime::SourceIdentity {
            case_id: fixture.case_id.0.clone(),
            data_source_id: domain::DataSourceId(source.source_id),
            revision: source.revision,
            source_kind: domain::DataSourceKind::E01,
            path,
        },
        factory.clone(),
    );
    let mut reader = runtime.open_reader().unwrap();
    source_kernel::e01_reader_cache::evict_e01_plans_for_case(&fixture.case_id.0);
    drop(runtime);
    drop(factory);
    let mut actual = vec![0; bytes.len()];
    reader.read_exact(&mut actual).unwrap();
    assert_eq!(actual, bytes);
    invalidate_case(&fixture.case_id.0);
    assert!(reader.seek(SeekFrom::Start(0)).is_err());
    source_kernel::e01_reader_cache::clear_e01_reader_cache_for_case(&fixture.case_id.0);
}

#[test]
fn an_exhausted_range_view_still_checks_invalidation_on_empty_reads_and_eof_seeks() {
    let fixture = Fixture::new();
    let bytes = bytes();
    let source = fixture.source("source", &bytes, &[]);
    let cancel = AtomicBool::new(false);
    let plan = plan_registered_access(
        &fixture.conn,
        fixture.root.path(),
        RegisteredAccessRequest {
            demand: demand(&source, bytes.len() as u64, &[(0, 16)], "preview"),
            file_id: None,
        },
        &cancel,
    )
    .unwrap();
    let mut reader = open_verified_view(plan, &cancel).unwrap();
    reader.read_exact(&mut [0; 16]).unwrap();
    invalidate_source(&fixture.case_id.0, "source");
    assert!(reader.read(&mut []).is_err());
    assert!(reader.seek(SeekFrom::End(0)).is_err());
}

#[test]
fn one_deep_window_verifies_shared_physical_dependencies_once_for_multiple_targets() {
    let fixture = Fixture::new();
    let bytes = bytes();
    let first = fixture.source("first", &bytes, &[]);
    let second = fixture.source("second", &bytes, &[]);
    let child = fixture.source("child", &bytes[..4096], &[first.clone(), second.clone()]);
    let batch = VerificationBatch::new_deep(Some(attempt())).unwrap();
    for (source, content) in [
        (&child, &bytes[..4096]),
        (&first, bytes.as_slice()),
        (&second, bytes.as_slice()),
    ] {
        let request = RegisteredAccessRequest {
            demand: demand(
                source,
                content.len() as u64,
                &[(0, content.len() as u64)],
                "deep",
            ),
            file_id: None,
        };
        verify_registered_access(
            &fixture.conn,
            fixture.root.path(),
            request,
            &batch,
            "registered-source/1",
            Some(&hex::encode(Sha256::digest(content))),
            false,
            &AtomicBool::new(false),
        )
        .unwrap();
    }
    assert_eq!(
        batch.stats().unwrap().verified_bytes,
        (2 * bytes.len() + 4096) as u64
    );
    assert_eq!(batch.stats().unwrap().reused_receipts, 2);
}
