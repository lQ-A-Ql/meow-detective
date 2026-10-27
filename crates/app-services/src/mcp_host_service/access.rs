use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use subtle::ConstantTimeEq;
use transport::dto::mcp_host::access::{
    McpHostClientDto, McpHostCreateClientRequestDto, McpHostCreatedClientDto,
    McpHostCredentialRecordDto,
};
use transport::dto::mcp_host::McpHostToolDto;

#[derive(Debug, thiserror::Error)]
pub enum McpAccessError {
    #[error("客户端授权无效或已过期")]
    Unauthorized,
    #[error("客户端名称、有效期或授权数量无效")]
    Invalid,
    #[error("无法生成客户端凭据")]
    Random,
}

impl transport::ServiceErrorCategory for McpAccessError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Unauthorized => transport::ErrorCategory::Security,
            Self::Invalid => transport::ErrorCategory::Validation,
            Self::Random => transport::ErrorCategory::Internal,
        }
    }
}

#[derive(Clone)]
pub struct McpPrincipal {
    pub client: McpHostClientDto,
    revoked: Arc<AtomicBool>,
}

impl McpPrincipal {
    pub fn active(&self) -> bool {
        !self.revoked.load(Ordering::Acquire)
            && self.client.expires_at > chrono::Utc::now().timestamp()
    }

    pub fn allows(&self, tool: &McpHostToolDto) -> bool {
        self.active()
            && tool.read_only
            && (tool.name != "forensics.read_file" || self.client.read_evidence)
    }

    pub fn allows_arguments(&self, tool: &McpHostToolDto, arguments: &serde_json::Value) -> bool {
        if !self.allows(tool) {
            return false;
        }
        let source_id = arguments
            .get("dataSourceId")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                arguments
                    .get("fileId")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|file_id| {
                        crate::source_db::GlobalFileId::parse(&domain::FileEntryId(
                            file_id.to_owned(),
                        ))
                        .ok()
                        .map(|id| id.data_source_id.0)
                    })
            });
        source_id.is_none_or(|id| {
            self.client.allowed_data_source_ids.is_empty()
                || self
                    .client
                    .allowed_data_source_ids
                    .iter()
                    .any(|allowed| allowed == &id)
        })
    }
}

#[derive(Default)]
pub struct McpAccessRegistry {
    entries: Vec<(McpHostCredentialRecordDto, Arc<AtomicBool>)>,
}

impl McpAccessRegistry {
    pub fn load(records: Vec<McpHostCredentialRecordDto>) -> Result<Self, McpAccessError> {
        if records.len() > 128
            || records.iter().any(|r| {
                r.client.id.is_empty()
                    || r.client.name.trim().is_empty()
                    || r.client.name.len() > 100
                    || r.client.name.chars().any(char::is_control)
                    || r.client.case_id.is_empty()
                    || r.client.case_session_id.is_empty()
                    || r.client.allowed_data_source_ids.len() > 64
                    || r.client
                        .allowed_data_source_ids
                        .iter()
                        .any(|id| !is_source_id(id))
            })
        {
            return Err(McpAccessError::Invalid);
        }
        let mut ids = std::collections::HashSet::new();
        if records.iter().any(|r| !ids.insert(&r.client.id)) {
            return Err(McpAccessError::Invalid);
        }
        Ok(Self {
            entries: records
                .into_iter()
                .map(|r| (r, Arc::new(AtomicBool::new(false))))
                .collect(),
        })
    }

    pub fn records(&self) -> Vec<McpHostCredentialRecordDto> {
        self.entries.iter().map(|(r, _)| r.clone()).collect()
    }

    pub fn clients(&self) -> Vec<McpHostClientDto> {
        self.entries.iter().map(|(r, _)| r.client.clone()).collect()
    }

    pub fn prepare(
        &self,
        request: McpHostCreateClientRequestDto,
        case_id: &str,
        session_id: &str,
    ) -> Result<(McpHostCreatedClientDto, McpHostCredentialRecordDto), McpAccessError> {
        if request.name.trim().is_empty()
            || request.name.len() > 100
            || request.name.chars().any(char::is_control)
            || case_id.is_empty()
            || session_id.is_empty()
            || !(1..=720).contains(&request.expires_after_hours)
            || request.allowed_data_source_ids.len() > 64
            || request
                .allowed_data_source_ids
                .iter()
                .any(|id| !is_source_id(id))
            || (request.read_evidence && request.allowed_data_source_ids.is_empty())
            || self.entries.len() >= 128
        {
            return Err(McpAccessError::Invalid);
        }
        let mut bytes = [0u8; 32];
        OsRng
            .try_fill_bytes(&mut bytes)
            .map_err(|_| McpAccessError::Random)?;
        let token = URL_SAFE_NO_PAD.encode(bytes);
        let client = McpHostClientDto {
            id: uuid::Uuid::new_v4().to_string(),
            name: request.name.trim().into(),
            case_id: case_id.into(),
            case_session_id: session_id.into(),
            expires_at: chrono::Utc::now().timestamp()
                + i64::from(request.expires_after_hours) * 3600,
            read_evidence: request.read_evidence,
            allowed_data_source_ids: request.allowed_data_source_ids,
        };
        let record = McpHostCredentialRecordDto {
            client: client.clone(),
            token_digest: Sha256::digest(token.as_bytes()).into(),
        };
        Ok((McpHostCreatedClientDto { client, token }, record))
    }

    pub fn insert(&mut self, record: McpHostCredentialRecordDto) {
        self.entries
            .push((record, Arc::new(AtomicBool::new(false))));
    }

    pub fn revoke(&mut self, id: &str) {
        self.entries.retain(|(record, revoked)| {
            if record.client.id != id {
                return true;
            }
            revoked.store(true, Ordering::Release);
            false
        });
    }

    pub fn authenticate(&self, token: &str) -> Result<McpPrincipal, McpAccessError> {
        if token.len() != 43 {
            return Err(McpAccessError::Unauthorized);
        }
        let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        self.entries
            .iter()
            .find_map(|(record, revoked)| {
                if !bool::from(record.token_digest.ct_eq(&digest)) {
                    return None;
                }
                let principal = McpPrincipal {
                    client: record.client.clone(),
                    revoked: revoked.clone(),
                };
                principal.active().then_some(principal)
            })
            .ok_or(McpAccessError::Unauthorized)
    }
}

fn is_source_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
}
