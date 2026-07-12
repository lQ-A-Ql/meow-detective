use super::TxlogTimestampInfo;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SecurityPolicyEntry {
    pub domain_name: Option<String>,
    pub account_domain_name: Option<String>,
    pub machine_sid: Option<String>,
    pub audit_policy_hex: Option<String>,
    pub source_key_path: String,
    pub last_write: Option<String>,
    pub txlog_applied: bool,
    pub txlog_timestamps: Vec<TxlogTimestampInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LsaSecretEntry {
    pub secret_name: String,
    pub version: String,
    pub encrypted_blob_hex: String,
    pub source_key_path: String,
    pub last_write: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CachedCredentialEntry {
    pub entry_name: String,
    pub encrypted_blob_hex: String,
    pub source_key_path: String,
    pub last_write: Option<String>,
}
