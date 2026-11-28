use std::sync::atomic::{AtomicBool, Ordering};

use super::{parse_kubernetes_artifact_in_workspace, Result};

pub(crate) fn read_and_parse(
    context: &mut evidence_access::file_service::SourceReadContext<'_>,
    case_root: &std::path::Path,
    artifact: &super::KubernetesMemberArtifact,
    cancel: &AtomicBool,
) -> Result<(Option<super::KubernetesParsedArtifact>, String)> {
    let limit = if artifact.kind == super::KubernetesArtifactKind::EtcdBackend {
        512 * 1024 * 1024
    } else {
        16 * 1024 * 1024
    };
    if artifact.size > limit {
        return Err(super::KubernetesParserError::Limit {
            kind: "artifact bytes",
            actual: artifact.size as usize,
            max: limit as usize,
        }
        .into());
    }
    let bytes = evidence_access::file_service::read_file_bytes_for_case(
        context,
        &artifact.file_id,
        0,
        limit as u32,
    )?;
    if bytes.len() as u64 != artifact.size {
        return Err(super::KubernetesParserError::Truncated {
            offset: bytes.len() as u64,
            context: "Kubernetes artifact",
        }
        .into());
    }
    let digest = infrastructure::hashing::sha256_bytes(&bytes);
    let parsed = parse_kubernetes_artifact_in_workspace(
        artifact.kind,
        &bytes,
        &case_root.join("derived/parser-inputs/bolt"),
        cancel,
    );
    ensure_not_cancelled(cancel)?;
    Ok((parsed?, digest))
}

fn ensure_not_cancelled(cancel_token: &AtomicBool) -> Result<()> {
    if cancel_token.load(Ordering::Relaxed) {
        Err(super::ClusterServiceError::Cancelled)
    } else {
        Ok(())
    }
}
