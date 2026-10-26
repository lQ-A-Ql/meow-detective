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

    pub fn allows(&self, tool: &transport::dto::mcp_host::McpHostToolDto) -> bool {
        self.active()
            && tool.read_only
            && (tool.name != "forensics.read_file" || self.client.read_evidence)
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
