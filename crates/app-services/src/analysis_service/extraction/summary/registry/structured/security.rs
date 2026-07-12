use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::artifact_query::query_artifact_rows;
use crate::analysis_service::extraction::attr_mapping::{optional_string_attr, string_attr};
use rusqlite::Connection;
use transport::dto::{CachedCredentialDto, LsaSecretDto, SecurityPolicyDto};

pub(super) struct SecurityRegistryData {
    pub(super) security_policies: Vec<SecurityPolicyDto>,
    pub(super) lsa_secrets: Vec<LsaSecretDto>,
    pub(super) cached_credentials: Vec<CachedCredentialDto>,
}

impl SecurityRegistryData {
    pub(super) fn load(conn: &Connection) -> Result<Self, AnalysisServiceError> {
        let security_policies = load_security_policies(conn)?;
        let lsa_secrets = load_lsa_secrets(conn)?;
        let cached_credentials = load_cached_credentials(conn)?;
        Ok(Self {
            security_policies,
            lsa_secrets,
            cached_credentials,
        })
    }
}

fn load_security_policies(
    conn: &Connection,
) -> Result<Vec<SecurityPolicyDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistrySecurityPolicy"], 0, 10_000)?
            .into_iter()
            .map(|row| SecurityPolicyDto {
                domain_name: optional_string_attr(&row.attrs, "domainName"),
                account_domain_name: optional_string_attr(&row.attrs, "accountDomainName"),
                machine_sid: optional_string_attr(&row.attrs, "machineSid"),
                audit_policy_hex: optional_string_attr(&row.attrs, "auditPolicyHex"),
                source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
                last_write: optional_string_attr(&row.attrs, "lastWrite"),
            })
            .collect(),
    )
}

fn load_lsa_secrets(conn: &Connection) -> Result<Vec<LsaSecretDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistryLsaSecret"], 0, 10_000)?
            .into_iter()
            .map(|row| LsaSecretDto {
                secret_name: string_attr(&row.attrs, "secretName"),
                version: string_attr(&row.attrs, "version"),
                encrypted_blob_hex: string_attr(&row.attrs, "encryptedBlobHex"),
                source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
                last_write: optional_string_attr(&row.attrs, "lastWrite"),
            })
            .collect(),
    )
}

fn load_cached_credentials(
    conn: &Connection,
) -> Result<Vec<CachedCredentialDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistryCachedCredential"], 0, 10_000)?
            .into_iter()
            .map(|row| CachedCredentialDto {
                entry_name: string_attr(&row.attrs, "entryName"),
                encrypted_blob_hex: string_attr(&row.attrs, "encryptedBlobHex"),
                source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
                last_write: optional_string_attr(&row.attrs, "lastWrite"),
            })
            .collect(),
    )
}
