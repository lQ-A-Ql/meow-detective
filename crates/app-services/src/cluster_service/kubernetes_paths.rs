//! Bounded Kubernetes evidence-path classification.
//!
//! This module deliberately does not parse file contents. It gives the
//! cluster-level planner a deterministic inventory of paths that can be handed
//! to the content parsers after a member source has been imported.

pub(crate) const MAX_KUBERNETES_ARTIFACTS: usize = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum KubernetesArtifactKind {
    StaticPodManifest,
    Kubeconfig,
    KubeletConfig,
    EtcdBackend,
    EtcdWal,
    AuditLog,
    PodLog,
    ContainerLog,
    PodVolume,
    CniConfig,
    RuntimeMetadata,
    Certificate,
}

impl KubernetesArtifactKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StaticPodManifest => "static_pod_manifest",
            Self::Kubeconfig => "kubeconfig",
            Self::KubeletConfig => "kubelet_config",
            Self::EtcdBackend => "etcd_backend",
            Self::EtcdWal => "etcd_wal",
            Self::AuditLog => "audit_log",
            Self::PodLog => "pod_log",
            Self::ContainerLog => "container_log",
            Self::PodVolume => "pod_volume",
            Self::CniConfig => "cni_config",
            Self::RuntimeMetadata => "runtime_metadata",
            Self::Certificate => "certificate",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubernetesArtifactObservation {
    pub path: String,
    pub kind: KubernetesArtifactKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubernetesArtifactInventory {
    pub artifacts: Vec<KubernetesArtifactObservation>,
    pub truncated: bool,
}

pub fn classify_kubernetes_path(path: &str) -> Option<KubernetesArtifactKind> {
    let normalized = normalize_path(path);
    classify_normalized_path(&normalized)
}

pub fn discover_kubernetes_artifacts<'a, I>(paths: I) -> KubernetesArtifactInventory
where
    I: IntoIterator<Item = &'a str>,
{
    let mut artifacts = paths
        .into_iter()
        .filter_map(|path| {
            classify_kubernetes_path(path).map(|kind| KubernetesArtifactObservation {
                path: path.to_string(),
                kind,
            })
        })
        .collect::<Vec<_>>();

    artifacts.sort_by(|left, right| {
        normalize_path(&left.path)
            .cmp(&normalize_path(&right.path))
            .then_with(|| left.kind.cmp(&right.kind))
    });
    artifacts.dedup_by(|left, right| {
        normalize_path(&left.path) == normalize_path(&right.path) && left.kind == right.kind
    });

    let truncated = artifacts.len() > MAX_KUBERNETES_ARTIFACTS;
    artifacts.truncate(MAX_KUBERNETES_ARTIFACTS);
    KubernetesArtifactInventory {
        artifacts,
        truncated,
    }
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

fn classify_normalized_path(path: &str) -> Option<KubernetesArtifactKind> {
    if is_static_pod_manifest(path) {
        return Some(KubernetesArtifactKind::StaticPodManifest);
    }
    if is_etcd_wal(path) {
        return Some(KubernetesArtifactKind::EtcdWal);
    }
    if is_etcd_backend(path) {
        return Some(KubernetesArtifactKind::EtcdBackend);
    }
    if is_audit_log(path) {
        return Some(KubernetesArtifactKind::AuditLog);
    }
    if path.contains("/var/log/containers/") {
        return Some(KubernetesArtifactKind::ContainerLog);
    }
    if path.contains("/var/log/pods/") {
        return Some(KubernetesArtifactKind::PodLog);
    }
    if path.contains("/var/lib/kubelet/pods/") {
        return Some(KubernetesArtifactKind::PodVolume);
    }
    if is_kubelet_config(path) {
        return Some(KubernetesArtifactKind::KubeletConfig);
    }
    if is_certificate(path) {
        return Some(KubernetesArtifactKind::Certificate);
    }
    if is_kubeconfig(path) {
        return Some(KubernetesArtifactKind::Kubeconfig);
    }
    if path.contains("/etc/cni/net.d/") || path.contains("/var/lib/cni/") {
        return Some(KubernetesArtifactKind::CniConfig);
    }
    if is_runtime_metadata(path) {
        return Some(KubernetesArtifactKind::RuntimeMetadata);
    }
    None
}

fn is_static_pod_manifest(path: &str) -> bool {
    path.contains("/etc/kubernetes/manifests/") && has_yaml_extension(path)
}

fn is_etcd_backend(path: &str) -> bool {
    path.ends_with("/var/lib/etcd/member/snap/db")
        || path.contains("/var/lib/rancher/k3s/server/db/")
        || path.contains("/var/lib/rancher/rke2/server/db/")
}

fn is_etcd_wal(path: &str) -> bool {
    path.contains("/var/lib/etcd/member/wal/")
        || path.contains("/var/lib/rancher/k3s/server/db/etcd/") && path.ends_with(".wal")
        || path.contains("/var/lib/rancher/rke2/server/db/etcd/") && path.ends_with(".wal")
}

fn is_audit_log(path: &str) -> bool {
    path.contains("/var/log/kubernetes/audit.log")
        || path.contains("/var/log/kube-apiserver/audit.log")
}

fn is_kubelet_config(path: &str) -> bool {
    path.ends_with("/var/lib/kubelet/config.yaml")
        || path.ends_with("/var/lib/kubelet/kubelet-config.yaml")
        || path.ends_with("/var/lib/kubelet/kubeadm-flags.env")
        || path.ends_with("/var/lib/kubelet/kubelet-flags.env")
}

fn is_certificate(path: &str) -> bool {
    path.contains("/var/lib/kubelet/pki/")
        || path.contains("/etc/kubernetes/pki/")
            && (path.ends_with(".crt") || path.ends_with(".pem") || path.ends_with(".key"))
}

fn is_kubeconfig(path: &str) -> bool {
    path.contains("/etc/kubernetes/") && (path.ends_with(".conf") || path.ends_with("kubeconfig"))
}

fn is_runtime_metadata(path: &str) -> bool {
    path.ends_with("/io.containerd.metadata.v1.bolt/meta.db")
        || path.contains("/var/lib/containers/storage/overlay-containers/")
}

fn has_yaml_extension(path: &str) -> bool {
    path.ends_with(".yaml") || path.ends_with(".yml")
}

#[cfg(test)]
#[path = "../../tests/unit/cluster_service/kubernetes_paths.rs"]
mod tests;
