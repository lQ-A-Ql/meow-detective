use std::sync::{atomic::AtomicBool, Arc};

use domain::structure::{NativeVersion, StructureRelationKind};
use persistence_sqlite::repositories::storage_object_repo::StorageObjectRepo;
use serde_json::{json, Value};

use super::*;

#[test]
fn published_cephfs_version_binds_observed_pvc_and_rejects_receipt_changes() {
    let fixture = fixture();
    let result = materialize(&fixture, true);
    let workload_source = import_workload_observations(&fixture, CLUSTER_ID);
    let version = StorageObjectRepo::new(&fixture.case_conn)
        .version_for_source(&fixture.case_id.0, &result.data_source.id.0)
        .unwrap()
        .unwrap();
    assert_eq!(version.acquisition_ids.len(), 3);
    assert_eq!(
        version.native_object_id,
        ceph_reconstruction::ceph_reconstruction::cephfs_native_object_id(
            CEPH_SCOPE_ID,
            CLUSTER_ID,
            1,
        )
    );
    assert_eq!(version.parent_source_revisions.len(), 3);
    assert!(matches!(
        &version.native_versions[0],
        NativeVersion::CephFs {
            filesystem_id: 1,
            fsmap_epoch: 17,
            mdsmap_epoch: 19,
            ..
        }
    ));
    let selected = vec![
        result.data_source.id.0.clone(),
        workload_source.id.0.clone(),
    ];
    let graph = source_reconstruction::structure_analysis::get_structure_graph(
        &fixture.case_conn,
        fixture.root.path(),
        &source_reconstruction::structure_analysis::StructureGraphRequest {
            case_id: fixture.case_id.clone(),
            source_ids: selected,
        },
    )
    .unwrap();
    let binding = graph
        .nodes
        .iter()
        .find(|node| node.details["relationKind"] == "claim_backend_binding")
        .expect("PVC and PV observations bind to an available published version");
    assert_eq!(
        binding.details["targetId"],
        format!("version:{}", version.id.0)
    );
    assert_eq!(
        binding.details["attributes"]["contentCapability"],
        "bounded_preview"
    );
    assert!(binding.details["attributes"]["publicationReceipt"]
        .as_str()
        .is_some());
    assert!(binding.details["attributes"]["claimEvidence"]["fileId"]
        .as_str()
        .is_some());
    assert_eq!(
        binding.details["attributes"]["historicalContentAtObservation"],
        "unproven"
    );
    assert!(graph
        .edges
        .iter()
        .any(|edge| edge.kind == StructureRelationKind::ContentDependency
            && edge.role == "reads_published_version"));
    assert!(graph
        .nodes
        .iter()
        .any(|node| node.details["relationKind"] == "declares_claim"));
    assert!(!graph
        .nodes
        .iter()
        .any(|node| node.details["entityKind"] == "pod"
            && graph.edges.iter().any(|edge| edge.from == node.id
                && edge.kind == StructureRelationKind::ContentDependency)));
    let read_only = app_services::infrastructure_graph_service::get_infrastructure_graph_read_only(
        &fixture.case_conn,
        fixture.root.path(),
        &fixture.case_id,
    )
    .unwrap();
    let writable = app_services::infrastructure_graph_service::get_infrastructure_graph(
        &fixture.case_conn,
        fixture.root.path(),
        &fixture.case_id,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(read_only).unwrap(),
        serde_json::to_value(writable).unwrap()
    );
    let repo = StructureViewAttemptRepo::new(&fixture.case_conn);
    let mut attempt = repo
        .find(CASE_ID, &result.data_source.id.0)
        .unwrap()
        .unwrap();
    attempt.receipt = if attempt.receipt == "f".repeat(64) {
        "e".repeat(64)
    } else {
        "f".repeat(64)
    };
    fixture
        .case_conn
        .execute(
            "DELETE FROM structure_view_attempts WHERE derived_source_id=?1",
            [&result.data_source.id.0],
        )
        .unwrap();
    repo.bind(CASE_ID, &attempt).unwrap();
    assert!(
        app_services::infrastructure_graph_service::get_infrastructure_graph_read_only(
            &fixture.case_conn,
            fixture.root.path(),
            &fixture.case_id,
        )
        .is_err()
    );
}

#[test]
fn browseable_namespace_without_file_content_keeps_volume_binding_unavailable() {
    let fixture = fixture();
    let mut namespace = namespace(true);
    namespace.entries[0].layout =
        Some(ceph_wire::CephFsFileLayout::new(65_536, 1, 65_536, 8, "").unwrap());
    let result = materialize_result_with_content(
        &fixture,
        &namespace,
        &present_assessment(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(
        result.capability,
        app_services::ceph_reconstruction::CephFsSourceCapability::MetadataBrowseable
    );
    let workload_source = import_workload_observations(&fixture, CLUSTER_ID);
    let graph = source_reconstruction::structure_facts::get_structure_fact_graph(
        &fixture.case_conn,
        fixture.root.path(),
        &fixture.case_id,
        &[result.data_source.id.0, workload_source.id.0],
    )
    .unwrap();
    assert!(graph
        .nodes
        .iter()
        .any(|node| node.details["relationKind"] == "backend_binding"
            && node.details["attributes"]["backendState"] == "needs_evidence"));
    assert!(!graph
        .edges
        .iter()
        .any(|edge| edge.kind == StructureRelationKind::ContentDependency));
}

#[test]
fn configuration_cluster_alias_does_not_identify_a_native_ceph_backend() {
    let fixture = fixture();
    let result = materialize(&fixture, true);
    let workload = import_workload_observations(&fixture, "cluster-a");
    let graph = source_reconstruction::structure_facts::get_structure_fact_graph(
        &fixture.case_conn,
        fixture.root.path(),
        &fixture.case_id,
        &[result.data_source.id.0, workload.id.0],
    )
    .unwrap();
    assert!(!graph
        .edges
        .iter()
        .any(|edge| edge.kind == StructureRelationKind::ContentDependency));
    assert!(graph
        .nodes
        .iter()
        .any(|node| node.details["relationKind"] == "backend_binding"
            && node.details["attributes"]["backendState"] == "needs_evidence"));
}

fn import_workload_observations(fixture: &Fixture, cluster_id: &str) -> DataSource {
    let root = fixture.root.path().join("workload-evidence");
    let audit = root.join("var/log/kubernetes/audit.log");
    std::fs::create_dir_all(audit.parent().unwrap()).unwrap();
    let pvc = json!({
        "apiVersion": "v1", "kind": "PersistentVolumeClaim",
        "metadata": {"uid":"claim-1","name":"files","namespace":"app","resourceVersion":"4"},
        "spec": {"volumeName":"volume-1"}, "status": {"phase":"Bound"}
    });
    let pv = json!({
        "apiVersion": "v1", "kind": "PersistentVolume",
        "metadata": {"uid":"pv-1","name":"volume-1","resourceVersion":"5"},
        "status": {"phase":"Bound"},
        "spec": {
            "claimRef": {"uid":"claim-1","name":"files","namespace":"app"},
            "csi": {"driver":"cephfs.csi.ceph.com","volumeHandle":"static-files",
                "volumeAttributes": {"clusterID":cluster_id,"fsName":"cephfs-a","rootPath":"/","staticVolume":"true"}}
        }
    });
    let pod = json!({
        "apiVersion":"v1","kind":"Pod",
        "metadata":{"uid":"pod-1","name":"reader","namespace":"app","resourceVersion":"6"},
        "spec":{"nodeName":"node-a","volumes":[{"name":"files","persistentVolumeClaim":{"claimName":"files"}}]}
    });
    let bytes = format!(
        "{}\n{}\n{}",
        audit_response(pvc, "2026-07-19T00:00:00Z"),
        audit_response(pv, "2026-07-19T00:00:01Z"),
        audit_response(pod, "2026-07-19T00:00:02Z")
    );
    std::fs::write(&audit, bytes).unwrap();
    let config = app_services::import_precheck::prepare_import_source_config(
        &root.to_string_lossy(),
        domain::DataSourcePlatform::Linux,
        None,
    )
    .unwrap();
    let job = persistence_sqlite::repositories::job_repo::JobRepo::new(&fixture.case_conn)
        .create(&fixture.case_id.0, "S04 workload evidence")
        .unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let (_, counts) = app_services::import_pipeline::execute_import_job_with_counts(
        &fixture.case_conn,
        &fixture.case_id,
        fixture.root.path(),
        config,
        &job,
        app_services::import_pipeline::ImportJobOptions {
            projection_policy: Default::default(),
            event_sink: None,
            cancel_token: &cancel,
            max_import_workers: Some(1),
            max_analysis_workers: Some(1),
            analysis_mode: app_services::import_analysis::ImportAnalysisMode::MetadataOnly,
            defer_linux_artifacts: false,
        },
    )
    .unwrap();
    assert_eq!(counts.failed_count, 0);
    DataSourceRepo::new(&fixture.case_conn)
        .find_by_case(&fixture.case_id)
        .unwrap()
        .into_iter()
        .find(|source| source.kind == DataSourceKind::LogicalDirectory)
        .unwrap()
}

fn audit_response(resource: Value, time: &str) -> Value {
    json!({
        "kind":"Event","apiVersion":"audit.k8s.io/v1","stage":"ResponseComplete",
        "auditID": format!("{}-{time}", resource["metadata"]["uid"].as_str().unwrap()),
        "requestReceivedTimestamp":time,"stageTimestamp":time,
        "verb":"get","responseStatus":{"code":200},"responseObject":resource
    })
}
