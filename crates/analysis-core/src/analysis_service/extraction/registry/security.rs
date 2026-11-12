use crate::analysis_service::artifact_builders::{base_attrs, make_artifact};
use crate::analysis_service::candidates::EvidenceCandidate;
use domain::Artifact;
use serde_json::Value;

pub(super) fn lsa_secret_artifacts(
    candidate: &EvidenceCandidate,
    entries: &[artifacts_windows::LsaSecretEntry],
) -> Vec<Artifact> {
    entries
        .iter()
        .map(|entry| {
            let mut attrs = base_attrs(candidate);
            attrs.insert(
                "secretName".to_string(),
                Value::String(entry.secret_name.clone()),
            );
            attrs.insert("version".to_string(), Value::String(entry.version.clone()));
            attrs.insert(
                "encryptedBlobHex".to_string(),
                Value::String(entry.encrypted_blob_hex.clone()),
            );
            attrs.insert(
                "sourceKeyPath".to_string(),
                Value::String(entry.source_key_path.clone()),
            );
            if let Some(ts) = entry.last_write.as_ref() {
                attrs.insert("lastWrite".to_string(), Value::String(ts.clone()));
            }
            attrs.insert(
                "parser".to_string(),
                Value::String("registry.security.lsasecret".to_string()),
            );
            make_artifact(
                "RegistryLsaSecret",
                format!("LSA Secret: {}", entry.secret_name),
                format!(
                    "{} {} ({} bytes encrypted blob)",
                    entry.secret_name,
                    entry.version,
                    entry.encrypted_blob_hex.len() / 2
                ),
                candidate,
                "registry.security.lsasecret.v1",
                attrs,
            )
        })
        .collect()
}

pub(super) fn cached_credential_artifacts(
    candidate: &EvidenceCandidate,
    entries: &[artifacts_windows::CachedCredentialEntry],
) -> Vec<Artifact> {
    entries
        .iter()
        .map(|entry| {
            let mut attrs = base_attrs(candidate);
            attrs.insert(
                "entryName".to_string(),
                Value::String(entry.entry_name.clone()),
            );
            attrs.insert(
                "encryptedBlobHex".to_string(),
                Value::String(entry.encrypted_blob_hex.clone()),
            );
            attrs.insert(
                "sourceKeyPath".to_string(),
                Value::String(entry.source_key_path.clone()),
            );
            if let Some(ts) = entry.last_write.as_ref() {
                attrs.insert("lastWrite".to_string(), Value::String(ts.clone()));
            }
            attrs.insert(
                "parser".to_string(),
                Value::String("registry.security.cachedcredential".to_string()),
            );
            make_artifact(
                "RegistryCachedCredential",
                format!("Cached Credential: {}", entry.entry_name),
                format!(
                    "{} ({} bytes encrypted blob)",
                    entry.entry_name,
                    entry.encrypted_blob_hex.len() / 2
                ),
                candidate,
                "registry.security.cachedcredential.v1",
                attrs,
            )
        })
        .collect()
}

pub(super) fn security_policy_artifacts(
    candidate: &EvidenceCandidate,
    entry: &artifacts_windows::SecurityPolicyEntry,
) -> Vec<Artifact> {
    let has_any = entry.domain_name.is_some()
        || entry.account_domain_name.is_some()
        || entry.machine_sid.is_some()
        || entry.audit_policy_hex.is_some();
    if !has_any {
        return Vec::new();
    }

    let mut attrs = base_attrs(candidate);
    if let Some(domain) = entry.domain_name.as_ref().filter(|s| !s.is_empty()) {
        attrs.insert("domainName".to_string(), Value::String(domain.clone()));
    }
    if let Some(account) = entry.account_domain_name.as_ref().filter(|s| !s.is_empty()) {
        attrs.insert(
            "accountDomainName".to_string(),
            Value::String(account.clone()),
        );
    }
    if let Some(sid) = entry.machine_sid.as_ref().filter(|s| !s.is_empty()) {
        attrs.insert("machineSid".to_string(), Value::String(sid.clone()));
    }
    if let Some(hex) = entry.audit_policy_hex.as_ref().filter(|s| !s.is_empty()) {
        attrs.insert("auditPolicyHex".to_string(), Value::String(hex.clone()));
    }
    attrs.insert(
        "sourceKeyPath".to_string(),
        Value::String(entry.source_key_path.clone()),
    );
    if let Some(ts) = entry.last_write.as_ref() {
        attrs.insert("lastWrite".to_string(), Value::String(ts.clone()));
    }
    attrs.insert(
        "parser".to_string(),
        Value::String("registry.security.policy".to_string()),
    );

    vec![make_artifact(
        "RegistrySecurityPolicy",
        "SECURITY Policy".to_string(),
        format!(
            "Domain: {}, Account domain: {}, SID: {}",
            entry.domain_name.as_deref().unwrap_or("-"),
            entry.account_domain_name.as_deref().unwrap_or("-"),
            entry.machine_sid.as_deref().unwrap_or("-")
        ),
        candidate,
        "registry.security.policy.v1",
        attrs,
    )]
}
