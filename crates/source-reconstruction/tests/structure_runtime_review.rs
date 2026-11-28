#[path = "support/structure_runtime_fixture.rs"]
mod fixture;

use std::{collections::BTreeSet, sync::atomic::AtomicBool};

use domain::structure::{
    ActivityState, CancellationScope, StructureRequest, TargetFailure, TargetStatus,
};
use persistence_sqlite::repositories::structure_activity_repo::{
    ActivityRecord, StructureActivityRepo,
};
use source_reconstruction::structure_runtime::{
    frontier::{CapabilityEntry, CapabilityIndex},
    registered_registry, registered_source_seed, run, source_scope_id, ActivityDefinition,
    BindingContext, BindingDecision, ExecutableRegistry, RuntimeError, RuntimeStart,
};

use fixture::Fixture;

const ATTEMPT: &str = "review-attempt";
const OWNER: &str = "review-owner";

#[test]
fn capability_capacity_exhaustion_returns_a_failed_activity_and_target_report() {
    let fixture = Fixture::new();
    let capture = fixture.source("capacity-input", b"real RAW bytes for capacity failure");
    let mut request = fixture.request(vec![capture.clone()]);
    request.resource_policy.active_work_items = 1;
    request.validate().unwrap();
    let scope = source_scope_id(&request).unwrap();
    let seed = registered_source_seed(&fixture.connection, &capture, &scope).unwrap();
    let registry = registered_registry().unwrap();
    let definitions = bound_source_activities(&request, &registry, std::slice::from_ref(&seed));
    assert_eq!(definitions.len(), 1);
    let expected_key = definitions[0].key().unwrap();
    let expected_target = definitions[0].target.identity().unwrap();
    let report = run(
        RuntimeStart {
            connection: &fixture.connection,
            case_root: fixture.root.path(),
            request: &request,
            registry: &registry,
            run_id: "capacity-review",
            attempt_id: ATTEMPT,
            owner_id: OWNER,
            cancel: &AtomicBool::new(false),
        },
        vec![seed],
        vec![],
    )
    .expect("capacity exhaustion must return a report with a typed target failure");
    assert_eq!(report.run_id, "capacity-review");
    assert_eq!(report.activities.len(), 1);
    assert_eq!(report.target_results.len(), 1);
    assert_eq!(report.pending_bindings, 0);
    let activity = &report.activities[0];
    assert_eq!(activity.work_key, expected_key);
    assert_eq!(activity.target_id, expected_target);
    assert_eq!(activity.state, ActivityState::Failed);
    assert_eq!(activity.output_receipt, None);
    assert!(matches!(
        activity.failure.as_ref(),
        Some(TargetFailure::ResourceLimit { required, limit, .. })
            if *required > *limit && *limit == 1
    ));
    let target = &report.target_results[0];
    assert_eq!(target.target_id, expected_target);
    assert_eq!(target.status, TargetStatus::NeedsEvidence);
    assert_eq!(target.error, activity.failure);
    assert!(target.evidence_ids.is_empty());
    assert!(!target.coverage.has_proven_portion().unwrap());
    assert_eq!(
        StructureActivityRepo::new(&fixture.connection)
            .get(&request.case_id, &report.run_id, &expected_key)
            .unwrap()
            .as_ref(),
        Some(activity)
    );
    assert_eq!(activity_count(&fixture, &report.run_id), 1);
    assert!(fixture.connection.is_autocommit());
}

#[test]
fn batch_owner_conflict_preserves_existing_activity_and_removes_all_new_batch_rows() {
    let fixture = Fixture::new();
    let first = fixture.source("batch-first", b"first real RAW input");
    let second = fixture.source("batch-second", b"second independent real RAW input");
    let request = fixture.request(vec![first.clone(), second.clone()]);
    let scope = source_scope_id(&request).unwrap();
    let seeds = [first, second]
        .iter()
        .map(|capture| registered_source_seed(&fixture.connection, capture, &scope).unwrap())
        .collect::<Vec<_>>();
    let registry = registered_registry().unwrap();
    let definitions = bound_source_activities(&request, &registry, &seeds);
    assert_eq!(definitions.len(), 2);
    let first_key = definitions[0].key().unwrap();
    let later_key = definitions[1].key().unwrap();
    assert!(first_key < later_key);
    let prior = ActivityRecord {
        case_id: request.case_id.clone(),
        run_id: "batch-review".into(),
        work_key: later_key.clone(),
        attempt_id: ATTEMPT.into(),
        owner_id: "other-owner".into(),
        definition_sha256: definitions[1].definition_digest().unwrap(),
        input_capability_ids: definitions[1].work.input_capability_versions.clone(),
        target_id: definitions[1].target.identity().unwrap(),
        state: ActivityState::Pending,
        output_receipt: None,
        failure: None,
    };
    let repository = StructureActivityRepo::new(&fixture.connection);
    assert!(repository.insert_pending(&prior).unwrap());
    assert!(repository
        .get(&request.case_id, &prior.run_id, &first_key)
        .unwrap()
        .is_none());
    let outcome = run(
        RuntimeStart {
            connection: &fixture.connection,
            case_root: fixture.root.path(),
            request: &request,
            registry: &registry,
            run_id: &prior.run_id,
            attempt_id: ATTEMPT,
            owner_id: OWNER,
            cancel: &AtomicBool::new(false),
        },
        seeds,
        definitions,
    );
    assert!(matches!(outcome, Err(RuntimeError::LostOwner)));
    assert!(repository
        .get(&request.case_id, &prior.run_id, &first_key)
        .unwrap()
        .is_none());
    assert_eq!(
        repository
            .get(&request.case_id, &prior.run_id, &later_key)
            .unwrap(),
        Some(prior.clone())
    );
    assert_eq!(activity_count(&fixture, &prior.run_id), 1);
    assert!(fixture.connection.is_autocommit());
}

#[test]
fn run_cancellation_marks_both_parent_and_dependent_cancelled_with_run_scope() {
    let fixture = Fixture::new();
    let first = fixture.source("cancel-parent", b"parent real RAW bytes");
    let second = fixture.source("cancel-dependent", b"dependent real RAW bytes");
    let request = fixture.request(vec![first.clone(), second.clone()]);
    let scope = source_scope_id(&request).unwrap();
    let seeds = [first, second]
        .iter()
        .map(|capture| registered_source_seed(&fixture.connection, capture, &scope).unwrap())
        .collect::<Vec<_>>();
    let registry = registered_registry().unwrap();
    let mut definitions = bound_source_activities(&request, &registry, &seeds);
    assert_eq!(definitions.len(), 2);
    let parent_key = definitions[0].key().unwrap();
    let dependent_key = definitions[1].key().unwrap();
    assert!(definitions[0].dependencies.is_empty());
    assert!(definitions[1].dependencies.insert(parent_key.clone()));
    definitions[1].validate().unwrap();
    assert_eq!(definitions[1].key().unwrap(), dependent_key);
    let target_ids = definitions
        .iter()
        .map(|definition| definition.target.identity().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(target_ids.len(), 2);
    let report = run(
        RuntimeStart {
            connection: &fixture.connection,
            case_root: fixture.root.path(),
            request: &request,
            registry: &registry,
            run_id: "cancel-chain-review",
            attempt_id: ATTEMPT,
            owner_id: OWNER,
            cancel: &AtomicBool::new(true),
        },
        seeds,
        definitions,
    )
    .expect("run cancellation must report every uncompleted activity");
    assert_eq!(report.activities.len(), 2);
    assert_eq!(report.target_results.len(), 2);
    assert_eq!(report.pending_bindings, 0);
    assert_eq!(
        report
            .activities
            .iter()
            .map(|activity| activity.work_key.clone())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([parent_key, dependent_key])
    );
    let reason = TargetFailure::Cancelled {
        scope: CancellationScope::Run,
    };
    let repository = StructureActivityRepo::new(&fixture.connection);
    for activity in &report.activities {
        assert_eq!(activity.state, ActivityState::Cancelled);
        assert_eq!(activity.failure.as_ref(), Some(&reason));
        assert_eq!(activity.output_receipt, None);
        assert_eq!(
            repository
                .get(&request.case_id, &report.run_id, &activity.work_key)
                .unwrap()
                .as_ref(),
            Some(activity)
        );
    }
    assert_eq!(
        report
            .target_results
            .iter()
            .map(|result| result.target_id.clone())
            .collect::<BTreeSet<_>>(),
        target_ids
    );
    for result in &report.target_results {
        assert_eq!(result.status, TargetStatus::Cancelled);
        assert_eq!(result.error.as_ref(), Some(&reason));
        assert!(result.evidence_ids.is_empty());
        assert!(!result.coverage.has_proven_portion().unwrap());
    }
    assert_eq!(activity_count(&fixture, &report.run_id), 2);
    assert!(fixture.connection.is_autocommit());
}

fn bound_source_activities(
    request: &StructureRequest,
    registry: &ExecutableRegistry,
    seeds: &[CapabilityEntry],
) -> Vec<ActivityDefinition> {
    let mut capabilities = CapabilityIndex::new(16).unwrap();
    for seed in seeds {
        assert!(capabilities.publish(seed.clone()).unwrap());
    }
    let mut definitions = Vec::new();
    for seed in seeds {
        let handlers = registry.matching(seed);
        assert_eq!(handlers.len(), 1);
        for handler in handlers {
            let BindingDecision::Bound(bound) = handler
                .bind(&BindingContext {
                    request,
                    changed: seed,
                    capabilities: &capabilities,
                })
                .unwrap()
            else {
                panic!("registered RAW source must bind its real range verification activity");
            };
            assert_eq!(bound.len(), 1);
            for definition in &bound {
                definition.validate().unwrap();
            }
            definitions.extend(bound);
        }
    }
    definitions.sort_by_cached_key(|definition| definition.key().unwrap());
    definitions
}

fn activity_count(fixture: &Fixture, run_id: &str) -> i64 {
    fixture
        .connection
        .query_row(
            "SELECT COUNT(*) FROM structure_activities WHERE case_id=?1 AND run_id=?2",
            rusqlite::params![fixture.case_id.0, run_id],
            |row| row.get(0),
        )
        .unwrap()
}
