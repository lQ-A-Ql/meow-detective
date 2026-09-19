use super::encoding::{append_field, append_optional_field};
use super::LEDGER_SCHEMA_VERSION;
use sha2::{Digest, Sha256};

pub const LEDGER_GENESIS_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerScope {
    Case,
    Global,
}

impl LedgerScope {
    pub fn key(self, case_id: Option<&str>) -> String {
        match (self, case_id) {
            (Self::Case, Some(case_id)) if !case_id.is_empty() => case_id.to_string(),
            (Self::Global, _) | (Self::Case, _) => "__global__".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForensicLedgerEvent {
    pub scope_key: String,
    pub case_id: Option<String>,
    pub audit_id: String,
    pub sequence: u64,
    pub actor_id: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub details: String,
    pub previous_hash: String,
    pub created_at: String,
}

impl ForensicLedgerEvent {
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(256);
        append_field(&mut bytes, LEDGER_SCHEMA_VERSION.as_bytes());
        append_field(&mut bytes, self.scope_key.as_bytes());
        append_optional_field(&mut bytes, self.case_id.as_deref());
        append_field(&mut bytes, self.audit_id.as_bytes());
        append_field(&mut bytes, &self.sequence.to_be_bytes());
        append_field(&mut bytes, self.actor_id.as_bytes());
        append_field(&mut bytes, self.action.as_bytes());
        append_field(&mut bytes, self.resource_type.as_bytes());
        append_optional_field(&mut bytes, self.resource_id.as_deref());
        append_field(&mut bytes, self.details.as_bytes());
        append_field(&mut bytes, self.previous_hash.as_bytes());
        append_field(&mut bytes, self.created_at.as_bytes());
        bytes
    }

    pub fn entry_hash(&self) -> String {
        hex::encode(Sha256::digest(self.canonical_bytes()))
    }
}
