use serde::{Deserialize, Serialize};

use crate::dto::{DigestAlgorithmDto, DigestScopeDto};

/// Requests a digest for an evidence object identified by its case-local ID.
///
/// Host paths are intentionally not accepted here. The backend resolves the
/// requested object from the active case and never reflects the resolved path.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalculateEvidenceDigestRequest {
    pub scope: DigestScopeDto,
    pub algorithm: DigestAlgorithmDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_source_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partition_index: Option<u32>,
}

impl CalculateEvidenceDigestRequest {
    pub fn validate(&self) -> Result<(), String> {
        let source_id = self.data_source_id.as_deref().unwrap_or_default();
        let file_id = self.file_id.as_deref().unwrap_or_default();
        if source_id.len() > 256 || file_id.len() > 512 {
            return Err("digest target identifier is too long".to_string());
        }
        match self.scope {
            DigestScopeDto::File if file_id.trim().is_empty() => {
                Err("fileId is required for file scope".to_string())
            }
            DigestScopeDto::ContainerFile | DigestScopeDto::ContainerSet
                if source_id.trim().is_empty() =>
            {
                Err("dataSourceId is required for container scope".to_string())
            }
            DigestScopeDto::Partition if source_id.trim().is_empty() => {
                Err("dataSourceId is required for partition scope".to_string())
            }
            DigestScopeDto::Partition if self.partition_index.is_none() => {
                Err("partitionIndex is required for partition scope".to_string())
            }
            DigestScopeDto::LogicalDisk if source_id.trim().is_empty() => {
                Err("dataSourceId is required for logical disk scope".to_string())
            }
            DigestScopeDto::DerivedEvidence if source_id.trim().is_empty() => {
                Err("dataSourceId is required for derived evidence scope".to_string())
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/commands/digest.rs"]
mod tests;
