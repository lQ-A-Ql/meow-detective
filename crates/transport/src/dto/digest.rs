use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DigestScopeDto {
    ContainerFile,
    ContainerSet,
    LogicalDisk,
    Partition,
    File,
    DerivedEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DigestAlgorithmDto {
    Md5,
    Sha1,
    Sha256,
    Sm3,
}

impl DigestAlgorithmDto {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Md5 => "md5",
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
            Self::Sm3 => "sm3",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DigestStatusDto {
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceDigestDto {
    pub scope: DigestScopeDto,
    pub algorithm: DigestAlgorithmDto,
    pub value: String,
    pub byte_length: u64,
    pub status: DigestStatusDto,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/digest.rs"]
mod tests;
