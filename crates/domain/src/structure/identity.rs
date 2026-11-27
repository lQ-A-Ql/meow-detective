use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StructureIdentityError {
    #[error("structure identity field '{field}' is empty")]
    EmptyField { field: &'static str },
    #[error("source digest is invalid")]
    InvalidDigest,
    #[error("source digest status does not match its value")]
    InconsistentDigestStatus { field: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AcquisitionId(pub String);

impl AcquisitionId {
    pub fn new(value: impl Into<String>) -> Result<Self, StructureIdentityError> {
        let value = value.into();
        if value.is_empty() {
            return Err(StructureIdentityError::EmptyField {
                field: "acquisition_id",
            });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRevisionRef {
    pub case_id: String,
    pub source_id: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRevisionContent {
    pub logical_size: u64,
    pub metadata_identity: Option<String>,
    pub metadata_identity_status: DigestVerification,
    pub logical_sha256: Option<String>,
    pub logical_sha256_status: DigestVerification,
    pub container_sha256: Option<String>,
    pub container_sha256_status: DigestVerification,
    pub consistency: RevisionConsistency,
}

impl SourceRevisionContent {
    pub fn captured(
        logical_size: u64,
        metadata_identity: Option<String>,
        logical_sha256: String,
    ) -> Self {
        Self {
            logical_size,
            metadata_identity_status: if metadata_identity.is_some() {
                DigestVerification::Recorded
            } else {
                DigestVerification::Missing
            },
            metadata_identity,
            logical_sha256: Some(logical_sha256),
            logical_sha256_status: DigestVerification::Verified,
            container_sha256: None,
            container_sha256_status: DigestVerification::Missing,
            consistency: RevisionConsistency::Coherent,
        }
    }

    pub fn observed(logical_size: u64, metadata_identity: Option<String>) -> Self {
        let metadata_identity_status = if metadata_identity.is_some() {
            DigestVerification::Recorded
        } else {
            DigestVerification::Missing
        };
        Self {
            logical_size,
            metadata_identity,
            metadata_identity_status,
            logical_sha256: None,
            logical_sha256_status: DigestVerification::Missing,
            container_sha256: None,
            container_sha256_status: DigestVerification::Missing,
            consistency: if metadata_identity_status == DigestVerification::Missing {
                RevisionConsistency::Indeterminate
            } else {
                RevisionConsistency::Partial
            },
        }
    }

    pub fn derived(logical_size: u64, metadata_identity: String) -> Self {
        Self {
            logical_size,
            metadata_identity: Some(metadata_identity),
            metadata_identity_status: DigestVerification::Recorded,
            logical_sha256: None,
            logical_sha256_status: DigestVerification::Missing,
            container_sha256: None,
            container_sha256_status: DigestVerification::Missing,
            consistency: RevisionConsistency::Coherent,
        }
    }

    pub fn validate(&self) -> Result<(), StructureIdentityError> {
        validate_digest(
            self.logical_sha256.as_deref(),
            self.logical_sha256_status,
            "logical_sha256",
        )?;
        validate_digest(
            self.container_sha256.as_deref(),
            self.container_sha256_status,
            "container_sha256",
        )?;
        if self.metadata_identity.as_deref().is_some_and(str::is_empty)
            || self.metadata_identity.is_none()
                && self.metadata_identity_status != DigestVerification::Missing
            || self.metadata_identity.is_some()
                && self.metadata_identity_status == DigestVerification::Missing
        {
            return Err(StructureIdentityError::InconsistentDigestStatus {
                field: "metadata_identity",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DigestVerification {
    Missing,
    Recorded,
    Verified,
}

impl DigestVerification {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Recorded => "recorded",
            Self::Verified => "verified",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "missing" => Some(Self::Missing),
            "recorded" => Some(Self::Recorded),
            "verified" => Some(Self::Verified),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionConsistency {
    Coherent,
    Historical,
    Partial,
    Conflicted,
    Indeterminate,
}

impl RevisionConsistency {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Coherent => "coherent",
            Self::Historical => "historical",
            Self::Partial => "partial",
            Self::Conflicted => "conflicted",
            Self::Indeterminate => "indeterminate",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "coherent" => Some(Self::Coherent),
            "historical" => Some(Self::Historical),
            "partial" => Some(Self::Partial),
            "conflicted" => Some(Self::Conflicted),
            "indeterminate" => Some(Self::Indeterminate),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeObjectId {
    pub adapter: String,
    pub identity_space: String,
    pub native_id: String,
    pub scope: String,
}

impl NativeObjectId {
    pub fn new(
        adapter: impl Into<String>,
        identity_space: impl Into<String>,
        native_id: impl Into<String>,
        scope: impl Into<String>,
    ) -> Result<Self, StructureIdentityError> {
        let value = Self {
            adapter: adapter.into(),
            identity_space: identity_space.into(),
            native_id: native_id.into(),
            scope: scope.into(),
        };
        for (field, value) in [
            ("adapter", value.adapter.as_str()),
            ("identity_space", value.identity_space.as_str()),
            ("native_id", value.native_id.as_str()),
            ("scope", value.scope.as_str()),
        ] {
            if value.is_empty() {
                return Err(StructureIdentityError::EmptyField { field });
            }
        }
        Ok(value)
    }

    pub fn storage_object_id(&self) -> String {
        identity_digest(
            b"native-object-v1",
            [
                self.adapter.as_str(),
                self.identity_space.as_str(),
                self.native_id.as_str(),
                self.scope.as_str(),
            ],
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionMemberVerification {
    pub source: SourceRevisionRef,
    pub collection_fingerprint: String,
}

fn validate_digest(
    value: Option<&str>,
    status: DigestVerification,
    field: &'static str,
) -> Result<(), StructureIdentityError> {
    match (value, status) {
        (None, DigestVerification::Missing) => Ok(()),
        (Some(value), DigestVerification::Recorded | DigestVerification::Verified)
            if valid_sha256(value) =>
        {
            Ok(())
        }
        (Some(_), DigestVerification::Recorded | DigestVerification::Verified) => {
            Err(StructureIdentityError::InvalidDigest)
        }
        _ => Err(StructureIdentityError::InconsistentDigestStatus { field }),
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) fn identity_digest<'a>(
    domain: &[u8],
    values: impl IntoIterator<Item = &'a str>,
) -> String {
    let mut digest = Sha256::new();
    digest.update(domain);
    for value in values {
        digest.update((value.len() as u64).to_le_bytes());
        digest.update(value.as_bytes());
    }
    format!(
        "{}-{}",
        String::from_utf8_lossy(domain),
        hex::encode(digest.finalize())
    )
}
