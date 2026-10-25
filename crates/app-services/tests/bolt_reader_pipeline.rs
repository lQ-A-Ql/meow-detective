use app_services::{
    bolt_reader_service::{decode, BoltReadError, BoltReadMode},
    cluster_service::{
        parse_kubernetes_artifact_in_workspace, KubernetesArtifactKind, KubernetesParsedArtifact,
    },
};
use std::sync::atomic::AtomicBool;

fn fixture() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../containers-bolt/tests/fixtures/go-bbolt.db"
    ))
    .unwrap()
}

#[test]
fn shared_worker_routes_containerd_and_etcd_with_evidence_values_redacted() {
    let workspace = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(false);
    let bytes = fixture();
    let images = decode(
        workspace.path(),
        &bytes,
        BoltReadMode::ContainerdImages,
        &cancel,
    )
    .unwrap();
    assert_eq!(images.report().entries.len(), 4);
    assert_eq!(
        images
            .report()
            .entries
            .iter()
            .filter(|entry| entry.value.is_some())
            .count(),
        2
    );
    drop(images);
    let parsed = parse_kubernetes_artifact_in_workspace(
        KubernetesArtifactKind::EtcdBackend,
        &bytes,
        workspace.path(),
        &cancel,
    )
    .unwrap()
    .unwrap();
    let KubernetesParsedArtifact::EtcdBackend(summary) = parsed else {
        panic!("etcd summary")
    };
    assert_eq!(summary.txid, 3);
    assert_eq!(summary.key_count, 2058);
    assert!(summary
        .entries
        .iter()
        .filter(|entry| entry.value_length.is_some_and(|length| length > 0))
        .all(|entry| entry.value_redacted));
    assert_eq!(std::fs::read_dir(workspace.path()).unwrap().count(), 0);
}

#[test]
fn corrupted_metadata_and_cancelled_request_release_workspace_files() {
    let workspace = tempfile::tempdir().unwrap();
    let mut bytes = fixture();
    bytes[72] ^= 1;
    bytes[4096 + 72] ^= 1;
    assert!(matches!(
        decode(
            workspace.path(),
            &bytes,
            BoltReadMode::ContainerdImages,
            &AtomicBool::new(false)
        ),
        Err(BoltReadError::Decode)
    ));
    assert!(matches!(
        decode(
            workspace.path(),
            &fixture(),
            BoltReadMode::EtcdMetadata,
            &AtomicBool::new(true)
        ),
        Err(BoltReadError::Cancelled)
    ));
    assert_eq!(std::fs::read_dir(workspace.path()).unwrap().count(), 0);
}
