#[path = "structure_facts/fixture.rs"]
mod fixture;

use domain::structure::{StructureNodeKind, StructureRelationKind};
use serde_json::json;

use fixture::{audit_event, CaseFixture};

#[test]
fn identical_names_and_uuids_in_independent_acquisitions_remain_separate() {
    let fixture = CaseFixture::new();
    let resource = json!({"kind":"Namespace","metadata":{"name":"production","uid":"cloned-uuid"}});
    let bytes = serde_json::to_vec(&resource).unwrap();
    for source in ["cluster-a", "cluster-b"] {
        fixture.source(
            source,
            &[("etc/kubernetes/resources.json", bytes.as_slice())],
        );
    }
    let graph = fixture.graph();
    let namespaces = graph
        .nodes
        .iter()
        .filter(|node| node.kind == StructureNodeKind::Namespace)
        .collect::<Vec<_>>();
    assert_eq!(namespaces.len(), 2);
    assert_ne!(namespaces[0].id, namespaces[1].id);
    assert!(namespaces
        .iter()
        .all(|node| node.details["contentCapability"] == "metadata_only"));
    assert!(!graph
        .edges
        .iter()
        .any(|edge| edge.kind == StructureRelationKind::ContentDependency));
    let facts = graph
        .nodes
        .iter()
        .filter(|node| node.kind == StructureNodeKind::EvidenceFact);
    for fact in facts {
        assert!(fact.details["evidence"]["fileId"].as_str().is_some());
        assert!(fact.details["evidence"]["contentSha256"].as_str().is_some());
        assert!(fact.details["evidence"]["sourceRevision"]["revision"]
            .as_u64()
            .is_some());
    }
}

#[test]
fn shared_collection_acquisition_does_not_merge_independent_cluster_sources() {
    let fixture = CaseFixture::new();
    let resource = json!({"kind":"Namespace","metadata":{"name":"production","uid":"copied-uid"}});
    let bytes = serde_json::to_vec(&resource).unwrap();
    for source in ["cluster-a", "cluster-b"] {
        fixture.source(
            source,
            &[("etc/kubernetes/resources.json", bytes.as_slice())],
        );
        let repo = persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo::new(
            &fixture.connection,
        );
        let (_, mut revision) = repo.current(&fixture.case_id.0, source).unwrap();
        revision.acquisition_id = domain::structure::AcquisitionId("same-collection".into());
        repo.bind(&fixture.case_id.0, source, &revision, true)
            .unwrap();
    }
    let graph = fixture.graph();
    assert_eq!(
        graph
            .nodes
            .iter()
            .filter(|node| node.kind == StructureNodeKind::Namespace)
            .count(),
        2
    );
}

#[test]
fn matching_vm_names_and_identifiers_in_separate_sources_remain_candidates() {
    let fixture = CaseFixture::new();
    for source in ["pve-cluster-a", "pve-cluster-b"] {
        fixture.source(
            source,
            &[(
                "etc/pve/qemu-server/100.conf",
                b"name: database\nscsi0: storage:vm-100-disk-0\n",
            )],
        );
    }
    let graph = fixture.graph();
    let vms = graph
        .nodes
        .iter()
        .filter(|node| node.details["entityKind"] == "vm")
        .collect::<Vec<_>>();
    assert_eq!(vms.len(), 2);
    assert!(vms
        .iter()
        .all(|node| node.details["identityState"] == "candidate"));
    assert!(!graph
        .edges
        .iter()
        .any(|edge| edge.kind == StructureRelationKind::ContentDependency));
}

#[test]
fn two_disks_share_host_only_with_hardware_and_boot_evidence() {
    let fixture = CaseFixture::new();
    let files = fixture::host_files("board-1");
    fixture.source("disk-a", &files);
    fixture.source("disk-b", &files);
    fixture.source("unidentified-disk", &[]);
    let graph = fixture.graph();
    let hosts = graph
        .nodes
        .iter()
        .filter(|node| node.kind == StructureNodeKind::PhysicalHost)
        .collect::<Vec<_>>();
    assert_eq!(hosts.len(), 1);
    let sources = graph
        .edges
        .iter()
        .filter(|edge| edge.from == hosts[0].id && edge.role == "host_source")
        .map(|edge| edge.to.0.as_str())
        .collect::<Vec<_>>();
    assert_eq!(sources, vec!["source:disk-a@1", "source:disk-b@1"]);
}

#[test]
fn cloned_hardware_uuid_with_distinct_board_identity_is_conflicted() {
    let fixture = CaseFixture::new();
    fixture.source("original", &fixture::host_files("board-original"));
    fixture.source("clone", &fixture::host_files("board-clone"));
    let graph = fixture.graph();
    let hosts = graph
        .nodes
        .iter()
        .filter(|node| node.kind == StructureNodeKind::PhysicalHost)
        .collect::<Vec<_>>();
    assert_eq!(hosts.len(), 2);
    assert!(hosts
        .iter()
        .all(|node| node.details["identityState"] == "conflicted"));
}

#[test]
fn vm_pvc_and_csi_configuration_without_backend_has_no_content_edges() {
    let fixture = CaseFixture::new();
    let resources = json!({"kind":"List","items":[
        {"kind":"PersistentVolumeClaim","metadata":{"name":"data","namespace":"app","uid":"claim-1"},
         "spec":{"volumeName":"pv-1"}},
        {"kind":"PersistentVolume","metadata":{"name":"pv-1","uid":"volume-1"},
         "spec":{"claimRef":{"uid":"claim-1","namespace":"app","name":"data"},
                 "csi":{"driver":"cephfs.csi.ceph.com","volumeHandle":"opaque-handle",
                        "volumeAttributes":{"clusterID":"absent","fsName":"files","rootPath":"/","staticVolume":"true"}}}},
        {"kind":"Namespace","metadata":{"name":"app","uid":"namespace-1"}}
    ]});
    let bytes = serde_json::to_vec(&resources).unwrap();
    fixture.source(
        "node",
        &[
            ("etc/kubernetes/resources.json", bytes.as_slice()),
            (
                "etc/pve/qemu-server/100.conf",
                b"name: evidence-vm\nscsi0: local-lvm:vm-100-disk-0\n",
            ),
        ],
    );
    let graph = fixture.graph();
    assert!(graph
        .nodes
        .iter()
        .any(|node| node.details["relationKind"] == "declares_disk"));
    assert!(graph
        .nodes
        .iter()
        .any(|node| node.details["relationKind"] == "declares_backend"));
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
fn historical_workload_placements_preserve_times_and_report_simultaneous_conflicts() {
    let fixture = CaseFixture::new();
    let mut original = audit_event("node-a", "2026-11-27T10:00:00Z", "1");
    original["objectRef"]["uid"] = json!("original-pod");
    original["responseObject"]["metadata"]["uid"] = json!("original-pod");
    let events = [
        original,
        audit_event("node-b", "2026-11-27T11:00:00+00:00", "2"),
        audit_event("node-c", "2026-11-27T11:00:00Z", "3"),
    ]
    .into_iter()
    .map(|event| event.to_string())
    .collect::<Vec<_>>()
    .join("\n");
    fixture.source(
        "control-plane",
        &[("var/log/kubernetes/audit.log", events.as_bytes())],
    );
    let graph = fixture.graph();
    let facts = graph
        .nodes
        .iter()
        .filter(|node| node.details["relationKind"] == "scheduled_on")
        .collect::<Vec<_>>();
    assert_eq!(facts.len(), 3);
    assert_eq!(
        graph
            .nodes
            .iter()
            .filter(|node| node.details["entityKind"] == "pod")
            .count(),
        2
    );
    assert_eq!(
        graph
            .nodes
            .iter()
            .filter(|node| node.details["entityKind"] == "workload")
            .count(),
        1
    );
    let historical = facts
        .iter()
        .find(|node| node.details["attributes"]["nodeName"] == "node-a")
        .unwrap();
    assert_eq!(historical.details["status"], "observed");
    assert_eq!(
        historical.details["evidence"]["observedAt"],
        "2026-11-27T10:00:00.000000000Z"
    );
    for node in facts
        .iter()
        .filter(|node| node.details["attributes"]["nodeName"] != "node-a")
    {
        assert_eq!(node.details["status"], "conflicted");
        assert_eq!(node.details["conflictIds"].as_array().unwrap().len(), 2);
    }
    let again = fixture.graph();
    assert_eq!(graph, again);
}

#[test]
fn rejected_requests_and_missing_event_time_cannot_prove_current_placement() {
    let fixture = CaseFixture::new();
    let mut rejected = audit_event("node-a", "2026-11-27T10:00:00Z", "1");
    rejected["responseStatus"]["code"] = json!(403);
    let mut untimed = audit_event("node-b", "2026-11-27T11:00:00Z", "2");
    untimed.as_object_mut().unwrap().remove("stageTimestamp");
    let events = format!("{rejected}\n{untimed}");
    fixture.source(
        "control-plane",
        &[("var/log/kubernetes/audit.log", events.as_bytes())],
    );
    let graph = fixture.graph();
    let facts = graph
        .nodes
        .iter()
        .filter(|node| node.details["relationKind"] == "scheduled_on")
        .collect::<Vec<_>>();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].details["status"], "candidate");
    assert!(facts[0].details["evidence"]["observedAt"].is_null());
}

#[test]
fn invalid_audit_and_reversed_timestamps_fail_at_the_evidence_boundary() {
    let invalid = CaseFixture::new();
    invalid.source(
        "control-plane",
        &[("var/log/kubernetes/audit.log", b"{invalid-json}")],
    );
    assert!(invalid
        .try_graph()
        .unwrap_err()
        .to_string()
        .contains("incomplete"));
    let reversed = CaseFixture::new();
    let mut event = audit_event("node-a", "2026-11-27T10:00:00Z", "1");
    event["requestReceivedTimestamp"] = json!("2026-11-27T10:00:00.000000001Z");
    let bytes = event.to_string();
    reversed.source(
        "control-plane",
        &[("var/log/kubernetes/audit.log", bytes.as_bytes())],
    );
    assert!(reversed
        .try_graph()
        .unwrap_err()
        .to_string()
        .contains("reversed"));
}

#[test]
fn fact_graph_reads_do_not_modify_original_evidence_or_case_rows() {
    let fixture = CaseFixture::new();
    let bytes = audit_event("node-a", "2026-11-27T10:00:00Z", "1").to_string();
    fixture.source(
        "control-plane",
        &[("var/log/kubernetes/audit.log", bytes.as_bytes())],
    );
    let before = fixture.connection.total_changes();
    fixture.graph();
    assert_eq!(fixture.connection.total_changes(), before);
    let original = std::fs::read(
        fixture
            .root
            .path()
            .join("inputs/control-plane/var/log/kubernetes/audit.log"),
    )
    .unwrap();
    assert_eq!(original, bytes.as_bytes());
}

#[test]
fn audit_response_uid_must_match_the_recorded_resource_reference() {
    let fixture = CaseFixture::new();
    let mut event = audit_event("node-a", "2026-11-27T10:00:00Z", "1");
    event["responseObject"]["metadata"]["uid"] = json!("different-pod");
    let bytes = event.to_string();
    fixture.source(
        "control-plane",
        &[("var/log/kubernetes/audit.log", bytes.as_bytes())],
    );
    assert!(fixture
        .try_graph()
        .unwrap_err()
        .to_string()
        .contains("identity conflicts"));
}

#[test]
fn hardware_uuid_without_corroboration_cannot_merge_collection_members() {
    let fixture = CaseFixture::new();
    for source in ["disk-a", "disk-b"] {
        fixture.source(
            source,
            &[(
                "sys/class/dmi/id/product_uuid",
                b"699ead5e-5739-4a12-b3a9-61ca15bbad1f",
            )],
        );
        let repo = persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo::new(
            &fixture.connection,
        );
        let (_, mut revision) = repo.current(&fixture.case_id.0, source).unwrap();
        revision.acquisition_id = domain::structure::AcquisitionId("same-collection".into());
        repo.bind(&fixture.case_id.0, source, &revision, true)
            .unwrap();
    }
    let graph = fixture.graph();
    assert_eq!(
        graph
            .nodes
            .iter()
            .filter(|node| node.kind == StructureNodeKind::PhysicalHost)
            .count(),
        2
    );
}

#[test]
fn source_file_content_replacement_is_rejected_before_fact_publication() {
    let fixture = CaseFixture::new();
    let bytes = audit_event("node-a", "2026-11-27T10:00:00Z", "1").to_string();
    fixture.source(
        "control-plane",
        &[("var/log/kubernetes/audit.log", bytes.as_bytes())],
    );
    let replacement = bytes.replace("node-a", "node-b");
    std::fs::write(
        fixture
            .root
            .path()
            .join("inputs/control-plane/var/log/kubernetes/audit.log"),
        replacement,
    )
    .unwrap();
    assert!(fixture
        .try_graph()
        .unwrap_err()
        .to_string()
        .contains("digest changed"));
}
