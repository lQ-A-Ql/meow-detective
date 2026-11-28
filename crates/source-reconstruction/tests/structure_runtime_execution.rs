#[path = "support/structure_runtime_fixture.rs"]
mod fixture;

use std::{collections::BTreeSet, sync::atomic::AtomicBool};

use domain::structure::{ActivityState, StructureStage, TargetStatus};
use source_reconstruction::structure_runtime::{
    registered_registry, registered_source_seed, run, source_scope_id, RuntimeError, RuntimeStart,
};

use fixture::Fixture;

#[test]
fn verified_bytes_trigger_the_next_layer_once_and_reports_release_read_ownership() {
    let fixture = Fixture::new();
    let capture = fixture.source("input", b"actual range content for the common runtime");
    let request = fixture.request(vec![capture.clone()]);
    let scope = source_scope_id(&request).unwrap();
    let seed = registered_source_seed(&fixture.connection, &capture, &scope).unwrap();
    let registry = registered_registry().unwrap();
    let stages = registry
        .descriptors()
        .iter()
        .map(|descriptor| descriptor.stage)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        stages,
        BTreeSet::from([
            StructureStage::SourceFreeze,
            StructureStage::ReadVerification
        ])
    );
    let report = run(
        RuntimeStart {
            connection: &fixture.connection,
            case_root: fixture.root.path(),
            request: &request,
            registry: &registry,
            run_id: "read-run",
            attempt_id: "read-attempt",
            owner_id: "read-owner",
            cancel: &AtomicBool::new(false),
        },
        vec![seed.clone(), seed],
        vec![],
    )
    .unwrap();
    assert_eq!(report.activities.len(), 2);
    assert_eq!(
        report
            .activity_stages
            .values()
            .copied()
            .collect::<BTreeSet<_>>(),
        stages
    );
    assert!(report
        .activities
        .iter()
        .all(|activity| activity.state == ActivityState::Succeeded));
    assert_eq!(report.target_results.len(), 1);
    assert_eq!(report.target_results[0].status, TargetStatus::Verified);
    assert_eq!(report.pending_bindings, 0);
    let path = fixture.root.path().join("input.raw");
    std::fs::remove_file(&path).unwrap();
    assert!(!path.exists());
    assert_eq!(report.target_results[0].status, TargetStatus::Verified);
}

#[test]
fn changed_source_fails_its_target_while_independent_source_completes() {
    let fixture = Fixture::new();
    let changed = fixture.source("damaged", b"captured original bytes");
    let intact = fixture.source("intact", b"independent valid evidence bytes");
    let request = fixture.request(vec![changed.clone(), intact.clone()]);
    let scope = source_scope_id(&request).unwrap();
    let first = registered_source_seed(&fixture.connection, &changed, &scope).unwrap();
    let second = registered_source_seed(&fixture.connection, &intact, &scope).unwrap();
    std::fs::write(
        fixture.root.path().join("damaged.raw"),
        b"changed content bytes!",
    )
    .unwrap();
    let report = run(
        RuntimeStart {
            connection: &fixture.connection,
            case_root: fixture.root.path(),
            request: &request,
            registry: &registered_registry().unwrap(),
            run_id: "independent",
            attempt_id: "attempt",
            owner_id: "owner",
            cancel: &AtomicBool::new(false),
        },
        vec![first, second],
        vec![],
    )
    .unwrap();
    assert_eq!(report.activities.len(), 3);
    assert_eq!(
        report
            .activities
            .iter()
            .filter(|activity| activity.state == ActivityState::Succeeded)
            .count(),
        2
    );
    assert_eq!(
        report
            .activities
            .iter()
            .filter(|activity| activity.state == ActivityState::Failed)
            .count(),
        1
    );
    assert_eq!(
        report
            .target_results
            .iter()
            .filter(|result| result.status == TargetStatus::Verified)
            .count(),
        1
    );
    assert_eq!(
        report
            .target_results
            .iter()
            .filter(|result| result.error.is_some())
            .count(),
        1
    );
}

#[test]
fn run_cancellation_records_terminal_activity_without_claiming_verified_output() {
    let fixture = Fixture::new();
    let capture = fixture.source("input", b"cancelled real evidence");
    let request = fixture.request(vec![capture.clone()]);
    let scope = source_scope_id(&request).unwrap();
    let seed = registered_source_seed(&fixture.connection, &capture, &scope).unwrap();
    let report = run(
        RuntimeStart {
            connection: &fixture.connection,
            case_root: fixture.root.path(),
            request: &request,
            registry: &registered_registry().unwrap(),
            run_id: "cancelled",
            attempt_id: "attempt",
            owner_id: "owner",
            cancel: &AtomicBool::new(true),
        },
        vec![seed],
        vec![],
    )
    .unwrap();
    assert_eq!(report.activities.len(), 1);
    assert_eq!(report.activities[0].state, ActivityState::Cancelled);
    assert_eq!(report.target_results[0].status, TargetStatus::Cancelled);
    assert!(report.target_results[0].evidence_ids.is_empty());
}

#[test]
fn foreign_scope_and_unavailable_exact_version_are_rejected_without_activity_execution() {
    let fixture = Fixture::new();
    let capture = fixture.source("input", b"scoped evidence");
    let mut request = fixture.request(vec![capture.clone()]);
    let scope = source_scope_id(&request).unwrap();
    let mut seed = registered_source_seed(&fixture.connection, &capture, &scope).unwrap();
    seed.scope_id = "foreign-scope".into();
    let registry = registered_registry().unwrap();
    let cancel = AtomicBool::new(false);
    let result = run(
        RuntimeStart {
            connection: &fixture.connection,
            case_root: fixture.root.path(),
            request: &request,
            registry: &registry,
            run_id: "foreign",
            attempt_id: "attempt",
            owner_id: "owner",
            cancel: &cancel,
        },
        vec![seed],
        vec![],
    );
    assert!(matches!(result, Err(RuntimeError::InvalidDefinition)));
    request.version_selection = domain::structure::VersionSelection::Exact {
        view_id: "unavailable-version".into(),
    };
    let seed = registered_source_seed(&fixture.connection, &capture, &scope).unwrap();
    let result = run(
        RuntimeStart {
            connection: &fixture.connection,
            case_root: fixture.root.path(),
            request: &request,
            registry: &registry,
            run_id: "wrong-version",
            attempt_id: "attempt",
            owner_id: "owner",
            cancel: &cancel,
        },
        vec![seed],
        vec![],
    );
    assert!(matches!(result, Err(RuntimeError::InvalidDefinition)));
    let count: i64 = fixture
        .connection
        .query_row("SELECT COUNT(*) FROM structure_activities", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn real_receipt_rejects_expanded_coverage_foreign_views_and_unbound_receipt_references() {
    use domain::structure::{ByteRange, CapabilityIntegrity, CapabilityRef, TargetCoverage};
    use source_kernel::source_runtime::verification::VerificationBatch;
    use source_reconstruction::structure_runtime::{
        frontier::CapabilityIndex, BindingContext, BindingDecision, ExecutionContext,
    };
    use std::collections::BTreeMap;
    let fixture = Fixture::new();
    let capture = fixture.source("input", b"real proof for explicit output validation");
    let mut request = fixture.request(vec![capture.clone()]);
    let scope = source_scope_id(&request).unwrap();
    let seed = registered_source_seed(&fixture.connection, &capture, &scope).unwrap();
    request
        .targets
        .push(domain::structure::TargetSelector::Block {
            view_id: seed.capability.view_id.clone(),
            range: ByteRange::new(0, 8).unwrap(),
        });
    let mut index = CapabilityIndex::new(8).unwrap();
    index.publish(seed.clone()).unwrap();
    let registry = registered_registry().unwrap();
    let handler = registry
        .get("registered-source", "verify-captured-range")
        .unwrap();
    let BindingDecision::Bound(definitions) = handler
        .bind(&BindingContext {
            request: &request,
            changed: &seed,
            capabilities: &index,
        })
        .unwrap()
    else {
        panic!("registered input did not bind")
    };
    let activity = &definitions[0];
    let batch = VerificationBatch::new(Some(domain::structure::StructureAttemptRef {
        attempt_id: "attempt".into(),
        owner: "owner".into(),
    }))
    .unwrap();
    let context = ExecutionContext {
        connection: &fixture.connection,
        case_root: fixture.root.path(),
        request: &request,
        attempt_id: "attempt",
        owner_id: "owner",
        batch: &batch,
        input_proofs: &BTreeMap::new(),
        cancel: &AtomicBool::new(false),
    };
    let output = handler.execute(activity, &context).unwrap();
    output.validate(activity).unwrap();
    let original = output.capabilities[0].clone();
    let mut output = output;
    let TargetCoverage::Bytes(mut coverage) = original.capability.coverage.clone() else {
        unreachable!()
    };
    coverage.requested = vec![ByteRange::new(0, 16).unwrap()];
    coverage.verified = coverage.requested.clone();
    output.capabilities[0].capability = CapabilityRef::new(
        original.capability.target_id.clone(),
        original.capability.view_id.clone(),
        original.capability.kind,
        original.capability.revision,
        original.capability.integrity.clone(),
        TargetCoverage::Bytes(coverage),
    )
    .unwrap();
    assert!(matches!(
        output.validate(activity),
        Err(RuntimeError::InvalidOutput)
    ));
    output.capabilities[0] = original.clone();
    output.capabilities[0].capability.view_id = "foreign-view".into();
    assert!(output.validate(activity).is_err());
    output.capabilities[0] = original;
    let CapabilityIntegrity::ContentVerified { content_sha256, .. } =
        &mut output.capabilities[0].capability.integrity
    else {
        unreachable!()
    };
    *content_sha256 = "a".repeat(64);
    assert!(output.validate(activity).is_err());
}

#[test]
fn revoked_real_read_proof_returns_a_typed_failure_for_its_dependent_target() {
    use domain::structure::{ConflictReason, TargetFailure};
    use source_kernel::source_runtime::verification::VerificationBatch;
    use source_reconstruction::structure_runtime::{
        frontier::CapabilityIndex, BindingContext, BindingDecision, ExecutionContext,
    };
    use std::collections::BTreeMap;
    let fixture = Fixture::new();
    let capture = fixture.source("input", b"real data for source revocation");
    let request = fixture.request(vec![capture.clone()]);
    let seed = registered_source_seed(
        &fixture.connection,
        &capture,
        &source_scope_id(&request).unwrap(),
    )
    .unwrap();
    let registry = registered_registry().unwrap();
    let mut index = CapabilityIndex::new(8).unwrap();
    index.publish(seed.clone()).unwrap();
    let reader = registry
        .get("registered-source", "verify-captured-range")
        .unwrap();
    let BindingDecision::Bound(reads) = reader
        .bind(&BindingContext {
            request: &request,
            changed: &seed,
            capabilities: &index,
        })
        .unwrap()
    else {
        unreachable!()
    };
    let batch = VerificationBatch::new(Some(domain::structure::StructureAttemptRef {
        attempt_id: "attempt".into(),
        owner: "owner".into(),
    }))
    .unwrap();
    let mut proofs = BTreeMap::new();
    let cancel = AtomicBool::new(false);
    let mut context = ExecutionContext {
        connection: &fixture.connection,
        case_root: fixture.root.path(),
        request: &request,
        attempt_id: "attempt",
        owner_id: "owner",
        batch: &batch,
        input_proofs: &BTreeMap::new(),
        cancel: &cancel,
    };
    let output = reader.execute(&reads[0], &context).unwrap();
    let capability = output.capabilities[0].clone();
    index.publish(capability.clone()).unwrap();
    let completion = registry
        .get("registered-source", "complete-block-target")
        .unwrap();
    let BindingDecision::Bound(targets) = completion
        .bind(&BindingContext {
            request: &request,
            changed: &capability,
            capabilities: &index,
        })
        .unwrap()
    else {
        unreachable!()
    };
    proofs.insert(capability.capability.id.clone(), output.proof);
    context.input_proofs = &proofs;
    source_kernel::source_runtime::invalidate_source(
        &capture.source.case_id,
        &capture.source.source_id,
    );
    assert!(matches!(
        completion.execute(&targets[0], &context),
        Err(RuntimeError::Target(TargetFailure::Conflicted {
            reason: ConflictReason::InputChanged
        }))
    ));
}

#[test]
fn new_execution_attempt_keeps_stable_work_keys_and_uses_independent_receipts() {
    let fixture = Fixture::new();
    let capture = fixture.source("input", b"same acquired source in two independent attempts");
    let request = fixture.request(vec![capture.clone()]);
    let seed = registered_source_seed(
        &fixture.connection,
        &capture,
        &source_scope_id(&request).unwrap(),
    )
    .unwrap();
    let registry = registered_registry().unwrap();
    let cancel = AtomicBool::new(false);
    let mut reports = Vec::new();
    for (run_id, attempt_id, owner_id) in [
        ("run-a", "attempt-a", "owner-a"),
        ("run-b", "attempt-b", "owner-b"),
    ] {
        reports.push(
            run(
                RuntimeStart {
                    connection: &fixture.connection,
                    case_root: fixture.root.path(),
                    request: &request,
                    registry: &registry,
                    run_id,
                    attempt_id,
                    owner_id,
                    cancel: &cancel,
                },
                vec![seed.clone()],
                vec![],
            )
            .unwrap(),
        );
    }
    let keys = |report: &source_reconstruction::structure_runtime::RuntimeReport| {
        report
            .activities
            .iter()
            .map(|activity| activity.work_key.clone())
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(keys(&reports[0]), keys(&reports[1]));
    assert_eq!(reports[0].activities.len(), 2);
    assert_ne!(
        reports[0].target_results[0].evidence_ids,
        reports[1].target_results[0].evidence_ids
    );
    assert!(reports.iter().all(|report| report
        .activities
        .iter()
        .all(|activity| activity.state == ActivityState::Succeeded)));
}
