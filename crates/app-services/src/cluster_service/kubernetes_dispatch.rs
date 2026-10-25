use super::audit_parser::{parse_kubernetes_audit_log, KubernetesAuditParseResult};
use super::etcd_bolt::{parse_etcd_bolt_metadata, EtcdBoltSummary};
use super::etcd_wal::{parse_etcd_wal, EtcdWalSummary};
use super::kubeconfig_parser::{parse_kubeconfig, KubeconfigSummary};
use super::kubernetes_parser_error::{KubernetesParserError, Result};
use super::kubernetes_paths::KubernetesArtifactKind;
use super::manifest_parser::{parse_static_pod_manifests, StaticPodManifestSummary};
use super::network_parser::CniNetworkSummary;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KubernetesParsedArtifact {
    Kubeconfig(KubeconfigSummary),
    StaticPodManifest(Vec<StaticPodManifestSummary>),
    AuditLog(KubernetesAuditParseResult),
    EtcdBackend(EtcdBoltSummary),
    EtcdWal(EtcdWalSummary),
    CniNetwork(CniNetworkSummary),
}

pub fn parse_kubernetes_artifact(
    kind: KubernetesArtifactKind,
    bytes: &[u8],
) -> Result<Option<KubernetesParsedArtifact>> {
    parse_with_bolt(kind, bytes, || {
        Err(KubernetesParserError::WorkspaceRequired)
    })
}

pub fn parse_kubernetes_artifact_in_workspace(
    kind: KubernetesArtifactKind,
    bytes: &[u8],
    workspace: &std::path::Path,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Option<KubernetesParsedArtifact>> {
    parse_with_bolt(kind, bytes, || {
        parse_etcd_bolt_metadata(workspace, bytes, cancel)
    })
}

fn parse_with_bolt(
    kind: KubernetesArtifactKind,
    bytes: &[u8],
    bolt: impl FnOnce() -> Result<EtcdBoltSummary>,
) -> Result<Option<KubernetesParsedArtifact>> {
    match kind {
        KubernetesArtifactKind::Kubeconfig => Ok(Some(KubernetesParsedArtifact::Kubeconfig(
            parse_kubeconfig(text(bytes)?)?,
        ))),
        KubernetesArtifactKind::StaticPodManifest => Ok(Some(
            KubernetesParsedArtifact::StaticPodManifest(parse_static_pod_manifests(text(bytes)?)?),
        )),
        KubernetesArtifactKind::AuditLog => Ok(Some(KubernetesParsedArtifact::AuditLog(
            parse_kubernetes_audit_log(text(bytes)?)?,
        ))),
        KubernetesArtifactKind::EtcdBackend => {
            Ok(Some(KubernetesParsedArtifact::EtcdBackend(bolt()?)))
        }
        KubernetesArtifactKind::EtcdWal => Ok(Some(KubernetesParsedArtifact::EtcdWal(
            parse_etcd_wal(bytes)?,
        ))),
        KubernetesArtifactKind::CniConfig => Ok(Some(KubernetesParsedArtifact::CniNetwork(
            super::network_parser::parse_cni_config(bytes)?,
        ))),
        _ => Ok(None),
    }
}

fn text(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).map_err(|_| KubernetesParserError::InvalidText)
}
