use super::*;
use crate::analysis_service::artifact_builders::{base_attrs, make_artifact};

pub(in crate::analysis_service::extraction) fn is_key_path(path: &str) -> bool {
    path.rsplit('/').next().is_some_and(|name| {
        name.starts_with("ssh_host_") && (name.ends_with("_key") || name.ends_with("_key.pub"))
    })
}

pub(super) fn extract(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    outcome: &mut ExtractionOutcome,
) {
    let path = normalize_evidence_path(&candidate.path);
    if bytes.starts_with(b"-----BEGIN RSA PRIVATE KEY-----")
        || bytes.starts_with(b"-----BEGIN EC PRIVATE KEY-----")
    {
        outcome.diagnostic_counts.capability_limits = 1;
        outcome.warnings.push(format!(
            "{} uses a legacy PEM private-key encoding outside the SSH metadata provider",
            candidate.path
        ));
        return;
    }
    let parsed = if is_key_path(&path) {
        artifacts_linux::ssh::parse_ssh_key_metadata(bytes, path.ends_with(".pub"))
            .map(|key| vec![key])
    } else {
        let text = match std::str::from_utf8(bytes) {
            Ok(text) => text,
            Err(error) => {
                outcome
                    .warnings
                    .push(format!("{} SSH UTF-8 error: {error}", candidate.path));
                return;
            }
        };
        if path.contains("/authorized_keys") {
            artifacts_linux::ssh::parse_authorized_keys_metadata(text)
        } else {
            artifacts_linux::ssh::parse_known_hosts_metadata(text)
        }
    };
    match parsed {
        Ok(keys) => {
            for (index, key) in keys.into_iter().enumerate() {
                let mut attrs = base_attrs(candidate);
                let line = key
                    .configuration
                    .clone()
                    .unwrap_or_else(|| format!("{} {}", key.algorithm, key.fingerprint));
                attrs.insert("configKind".into(), "sshKey".into());
                attrs.insert("line".into(), line.into());
                attrs.insert("algorithm".into(), key.algorithm.into());
                attrs.insert("fingerprint".into(), key.fingerprint.into());
                attrs.insert("encrypted".into(), key.encrypted.into());
                attrs.insert("comment".into(), key.comment.into());
                if let Some(configuration) = key.configuration {
                    attrs.insert("configuration".into(), configuration.into());
                }
                attrs.insert("recordIndex".into(), index.into());
                outcome.artifacts.push(make_artifact(
                    "LinuxSystemConfig",
                    "SSH key metadata".into(),
                    candidate.path.clone(),
                    candidate,
                    "linux.ssh_key",
                    attrs,
                ));
            }
        }
        Err(error) => outcome
            .warnings
            .push(format!("{} SSH key parse failed: {error}", candidate.path)),
    }
}
