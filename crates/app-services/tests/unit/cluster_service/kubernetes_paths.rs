use crate::cluster_service::kubernetes_paths::{
    classify_kubernetes_path, discover_kubernetes_artifacts, KubernetesArtifactKind,
};

#[test]
fn classifies_control_plane_and_runtime_paths() {
    assert_eq!(
        classify_kubernetes_path("[P2]\\etc\\kubernetes\\manifests\\kube-apiserver.yaml"),
        Some(KubernetesArtifactKind::StaticPodManifest)
    );
    assert_eq!(
        classify_kubernetes_path("/var/lib/etcd/member/snap/db"),
        Some(KubernetesArtifactKind::EtcdBackend)
    );
    assert_eq!(
        classify_kubernetes_path("/var/log/containers/api-abc_default_api-123.log"),
        Some(KubernetesArtifactKind::ContainerLog)
    );
    assert_eq!(
        classify_kubernetes_path("/var/lib/kubelet/pods/uid/volumes/kubernetes.io~secret/token"),
        Some(KubernetesArtifactKind::PodVolume)
    );
}

#[test]
fn preserves_original_path_and_deduplicates_case_and_separator_variants() {
    let inventory = discover_kubernetes_artifacts([
        "\\etc\\kubernetes\\admin.conf",
        "/etc/kubernetes/admin.conf",
        "/etc/kubernetes/ADMIN.CONF",
        "/etc/kubernetes/not-a-kubernetes-file.txt",
    ]);

    assert_eq!(inventory.artifacts.len(), 1);
    assert_eq!(inventory.artifacts[0].path, "\\etc\\kubernetes\\admin.conf");
    assert!(!inventory.truncated);
}

#[test]
fn applies_a_bounded_inventory_limit() {
    let paths = (0..16_500)
        .map(|index| format!("/var/log/containers/pod-{index}.log"))
        .collect::<Vec<_>>();
    let path_refs = paths.iter().map(String::as_str).collect::<Vec<_>>();

    let inventory = discover_kubernetes_artifacts(path_refs);

    assert_eq!(inventory.artifacts.len(), 16_384);
    assert!(inventory.truncated);
}
