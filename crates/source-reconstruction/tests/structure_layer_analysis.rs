#[path = "support/structure_runtime_fixture.rs"]
mod fixture;
#[path = "support/structure_layer_fixture.rs"]
mod layers;

use std::sync::atomic::AtomicBool;

use domain::structure::{ActivityState, StructureGoal, TargetStatus};
use source_reconstruction::structure_runtime::{
    analysis_source_seeds, register_layer_analysis, run, source_scope_id, ExecutableRegistry,
    LayerAnalysisProfile, RuntimeStart,
};

use fixture::Fixture;

const MANIFEST: &[u8] = b"apiVersion: v1\nkind: Pod\nmetadata:\n  name: kube-apiserver\n  namespace: kube-system\nspec:\n  hostNetwork: true\n  containers:\n    - name: kube-apiserver\n      image: registry.k8s.io/kube-apiserver:v1.31.0\n      command:\n        - kube-apiserver\n        - --etcd-servers=https://127.0.0.1:2379\n";

#[test]
fn control_plane_metadata_analysis_completes_without_a_guest_platform_or_disk_backend() {
    let fixture = Fixture::new();
    let unrelated = fixture.source(
        "unrelated",
        b"another acquired source outside the selected scope",
    );
    let capture = layers::logical_source(
        &fixture,
        "control",
        &[("etc/kubernetes/manifests/kube-apiserver.yaml", MANIFEST)],
    );
    let mut request = fixture.request(vec![capture.clone()]);
    request.goals.insert(StructureGoal::Analyze);
    request
        .analysis_profiles
        .insert("kubernetes-metadata".into());
    let scope = source_scope_id(&request).unwrap();
    let seeds = analysis_source_seeds(
        &fixture.connection,
        fixture.root.path(),
        &capture,
        &scope,
        LayerAnalysisProfile::KubernetesMetadata,
        "control-plane",
    )
    .unwrap();
    let mut registry = ExecutableRegistry::new(16).unwrap();
    register_layer_analysis(&mut registry).unwrap();
    let report = run(
        RuntimeStart {
            connection: &fixture.connection,
            case_root: fixture.root.path(),
            request: &request,
            registry: &registry,
            run_id: "metadata-run",
            attempt_id: "metadata-attempt",
            owner_id: "metadata-owner",
            cancel: &AtomicBool::new(false),
        },
        seeds,
        vec![],
    )
    .unwrap();
    assert_eq!(report.activities.len(), 1);
    assert_eq!(report.activities[0].state, ActivityState::Succeeded);
    assert_eq!(report.target_results[0].status, TargetStatus::Verified);
    assert_eq!(report.analysis_results.len(), 1);
    let result = report.analysis_results.values().next().unwrap();
    assert_eq!(result.profile_id, "kubernetes-metadata");
    assert_eq!(result.source_revision, capture.source);
    assert_ne!(result.source_revision, unrelated.source);
    result.receipt.validate().unwrap();
    let encoded = serde_json::to_string(&result.details).unwrap();
    assert!(encoded.contains("kube-apiserver"));
    assert!(!encoded.contains(fixture.root.path().to_str().unwrap()));
    assert_eq!(
        infrastructure::hashing::sha256_bytes(&serde_json::to_vec(&result.details).unwrap()),
        result.receipt.output_sha256
    );
    let stored = persistence_source::analysis_results::read_analysis_result(
        fixture.root.path(),
        &capture.source,
        &result.storage.receipt_id,
    )
    .unwrap();
    assert_eq!(stored.output, result.details);
    assert_eq!(stored.receipt, result.receipt);
}

#[test]
fn snapshot_analysis_proof_rejects_generation_invalidation_and_releases_ownership() {
    use domain::structure::StructureAttemptRef;
    use source_kernel::source_runtime::snapshot_analysis::SnapshotAnalysisSession;
    use std::collections::BTreeSet;
    let fixture = Fixture::new();
    let capture = layers::logical_source(&fixture, "source", &[("etc/os-release", b"ID=debian\n")]);
    let session = SnapshotAnalysisSession::acquire(
        fixture.root.path(),
        &capture.source,
        "metadata-target".into(),
        BTreeSet::from(["input-capability".into()]),
        StructureAttemptRef {
            attempt_id: "attempt".into(),
            owner: "owner".into(),
        },
        "native-state-analysis".into(),
        "1".into(),
    )
    .unwrap();
    let proof = session.seal(&serde_json::json!({"id": "debian"})).unwrap();
    let report = proof.receipt().clone();
    proof.validate().unwrap();
    source_kernel::source_runtime::invalidate_source(
        &capture.source.case_id,
        &capture.source.source_id,
    );
    assert!(proof.validate().is_err());
    drop(proof);
    report.validate().unwrap();
}

#[test]
fn exact_analysis_view_filters_other_sources_and_keeps_one_summary_proof() {
    use domain::structure::VersionSelection;
    let fixture = Fixture::new();
    let first = layers::logical_source(
        &fixture,
        "first",
        &[("etc/kubernetes/manifests/kube-apiserver.yaml", MANIFEST)],
    );
    let second = layers::logical_source(
        &fixture,
        "second",
        &[("etc/kubernetes/manifests/kube-apiserver.yaml", MANIFEST)],
    );
    let mut request = fixture.request(vec![first.clone(), second.clone()]);
    request.goals.insert(StructureGoal::Analyze);
    request
        .analysis_profiles
        .insert("kubernetes-metadata".into());
    let scope = source_scope_id(&request).unwrap();
    let seed = |capture: &domain::structure::SourceCaptureRef| {
        analysis_source_seeds(
            &fixture.connection,
            fixture.root.path(),
            capture,
            &scope,
            LayerAnalysisProfile::KubernetesMetadata,
            "control-plane",
        )
        .unwrap()
    };
    let first_seed = seed(&first);
    let second_seed = seed(&second);
    request.version_selection = VersionSelection::Exact {
        view_id: first_seed[0].capability.view_id.clone(),
    };
    let mut registry = ExecutableRegistry::new(16).unwrap();
    register_layer_analysis(&mut registry).unwrap();
    let report = run(
        RuntimeStart {
            connection: &fixture.connection,
            case_root: fixture.root.path(),
            request: &request,
            registry: &registry,
            run_id: "exact-run",
            attempt_id: "attempt",
            owner_id: "owner",
            cancel: &AtomicBool::new(false),
        },
        second_seed.into_iter().chain(first_seed).collect(),
        vec![],
    )
    .unwrap();
    assert_eq!(report.activities.len(), 1);
    assert_eq!(
        report
            .analysis_results
            .values()
            .next()
            .unwrap()
            .source_revision,
        first.source
    );
}

#[test]
fn namespace_summary_rejects_changed_counts_and_unbound_page_proofs() {
    use source_kernel::source_runtime::verification::VerificationBatch;
    use source_reconstruction::structure_runtime::{
        frontier::CapabilityIndex, BindingContext, BindingDecision, ExecutionContext, RuntimeError,
    };
    use std::collections::BTreeMap;
    let fixture = Fixture::new();
    let capture = layers::logical_source(
        &fixture,
        "summary",
        &[("etc/kubernetes/manifests/kube-apiserver.yaml", MANIFEST)],
    );
    let mut request = fixture.request(vec![capture.clone()]);
    request.goals.insert(StructureGoal::Analyze);
    request
        .analysis_profiles
        .insert("kubernetes-metadata".into());
    let seed = analysis_source_seeds(
        &fixture.connection,
        fixture.root.path(),
        &capture,
        &source_scope_id(&request).unwrap(),
        LayerAnalysisProfile::KubernetesMetadata,
        "control-plane",
    )
    .unwrap()
    .remove(0);
    let mut index = CapabilityIndex::new(8).unwrap();
    index.publish(seed.clone()).unwrap();
    let mut registry = ExecutableRegistry::new(16).unwrap();
    register_layer_analysis(&mut registry).unwrap();
    let operation = registry
        .get("kubernetes-metadata", "analyze-published-native-state")
        .unwrap();
    let BindingDecision::Bound(activities) = operation
        .bind(&BindingContext {
            request: &request,
            changed: &seed,
            capabilities: &index,
        })
        .unwrap()
    else {
        unreachable!()
    };
    let batch = VerificationBatch::new(None).unwrap();
    let output = operation
        .execute(
            &activities[0],
            &ExecutionContext {
                connection: &fixture.connection,
                case_root: fixture.root.path(),
                request: &request,
                attempt_id: "attempt",
                owner_id: "owner",
                batch: &batch,
                input_proofs: &BTreeMap::new(),
                cancel: &AtomicBool::new(false),
            },
        )
        .unwrap();
    output.validate(&activities[0]).unwrap();
    let mut output = output;
    let domain::structure::TargetCoverage::Namespace(coverage) =
        &mut output.target_results[0].coverage
    else {
        unreachable!()
    };
    coverage.pages[0].object_count = 100;
    assert!(matches!(
        output.validate(&activities[0]),
        Err(RuntimeError::InvalidOutput)
    ));
}

#[test]
fn guest_linux_uses_shared_artifact_parser_and_unknown_guest_does_not_block_metadata() {
    let fixture = Fixture::new();
    let capture = layers::logical_source_with_platform(
        &fixture,
        "linux-guest",
        &[
            (
                "etc/os-release",
                b"NAME=\"Debian GNU/Linux\"\nID=debian\nVERSION_ID=\"12\"\n",
            ),
            (
                "etc/passwd",
                b"root:x:0:0:root:/root:/bin/bash\nalice:x:1000:1000:Alice:/home/alice:/bin/bash\n",
            ),
        ],
        Some("linux"),
    );
    let control = layers::logical_source(
        &fixture,
        "control-node",
        &[("etc/kubernetes/manifests/kube-apiserver.yaml", MANIFEST)],
    );
    let mut request = fixture.request(vec![capture.clone(), control.clone()]);
    request.goals.insert(StructureGoal::Analyze);
    request
        .analysis_profiles
        .extend(["guest-linux-artifacts".into(), "kubernetes-metadata".into()]);
    let scope = source_scope_id(&request).unwrap();
    let mut seeds = analysis_source_seeds(
        &fixture.connection,
        fixture.root.path(),
        &capture,
        &scope,
        LayerAnalysisProfile::GuestLinuxArtifacts,
        "guest-files",
    )
    .unwrap();
    seeds.extend(
        analysis_source_seeds(
            &fixture.connection,
            fixture.root.path(),
            &control,
            &scope,
            LayerAnalysisProfile::KubernetesMetadata,
            "control-plane",
        )
        .unwrap(),
    );
    let mut registry = ExecutableRegistry::new(16).unwrap();
    register_layer_analysis(&mut registry).unwrap();
    let report = run(
        RuntimeStart {
            connection: &fixture.connection,
            case_root: fixture.root.path(),
            request: &request,
            registry: &registry,
            run_id: "layer-combination",
            attempt_id: "attempt",
            owner_id: "owner",
            cancel: &AtomicBool::new(false),
        },
        seeds,
        vec![],
    )
    .unwrap();
    assert_eq!(report.activities.len(), 2);
    assert!(report
        .activities
        .iter()
        .all(|activity| activity.state == ActivityState::Succeeded));
    assert_eq!(report.analysis_results.len(), 2);
    let guest = report
        .analysis_results
        .values()
        .find(|result| result.profile_id == "guest-linux-artifacts")
        .unwrap();
    let encoded = serde_json::to_string(&guest.details).unwrap();
    assert!(encoded.contains("debian"));
    assert!(encoded.contains("alice"));
    let control = report
        .analysis_results
        .values()
        .find(|result| result.profile_id == "kubernetes-metadata")
        .unwrap();
    assert!(control.details["controlPlaneObserved"].as_bool().unwrap());
}

#[test]
fn member_metadata_waits_for_its_own_view_kv_evidence_and_preserves_an_independent_target() {
    use domain::structure::{CapabilityKind, TargetFailure};
    let fixture = Fixture::new();
    let member = layers::logical_source(
        &fixture,
        "member",
        &[("capture-note", b"scope-only dependency test")],
    );
    let other = layers::logical_source(
        &fixture,
        "other-member",
        &[("capture-note", b"independent capture")],
    );
    let control = layers::logical_source(
        &fixture,
        "control",
        &[("etc/kubernetes/manifests/kube-apiserver.yaml", MANIFEST)],
    );
    let mut request = fixture.request(vec![member.clone(), other.clone(), control.clone()]);
    request.goals.insert(StructureGoal::Analyze);
    request
        .analysis_profiles
        .extend(["ceph-osd-metadata".into(), "kubernetes-metadata".into()]);
    let scope = source_scope_id(&request).unwrap();
    let mut seeds = analysis_source_seeds(
        &fixture.connection,
        fixture.root.path(),
        &member,
        &scope,
        LayerAnalysisProfile::CephOsdMetadata,
        "osd-metadata",
    )
    .unwrap();
    seeds.retain(|entry| entry.capability.kind == CapabilityKind::MemberMetadata);
    seeds.extend(
        analysis_source_seeds(
            &fixture.connection,
            fixture.root.path(),
            &other,
            &scope,
            LayerAnalysisProfile::CephOsdMetadata,
            "osd-metadata",
        )
        .unwrap()
        .into_iter()
        .filter(|entry| entry.capability.kind == CapabilityKind::KvInventory),
    );
    seeds.extend(
        analysis_source_seeds(
            &fixture.connection,
            fixture.root.path(),
            &control,
            &scope,
            LayerAnalysisProfile::KubernetesMetadata,
            "control-plane",
        )
        .unwrap(),
    );
    let mut registry = ExecutableRegistry::new(16).unwrap();
    register_layer_analysis(&mut registry).unwrap();
    let report = run(
        RuntimeStart {
            connection: &fixture.connection,
            case_root: fixture.root.path(),
            request: &request,
            registry: &registry,
            run_id: "requirements",
            attempt_id: "attempt",
            owner_id: "owner",
            cancel: &AtomicBool::new(false),
        },
        seeds,
        vec![],
    )
    .unwrap();
    assert_eq!(report.pending_bindings, 1);
    assert_eq!(report.activities.len(), 1);
    assert_eq!(report.activities[0].state, ActivityState::Succeeded);
    assert_eq!(report.target_results.len(), 2);
    assert!(report.target_results.iter().any(|result| matches!(
        &result.error, Some(TargetFailure::MissingCapabilities { capabilities }) if capabilities == &std::collections::BTreeSet::from([CapabilityKind::KvInventory])
    )));
    assert_eq!(
        report.analysis_results.values().next().unwrap().profile_id,
        "kubernetes-metadata"
    );
}
