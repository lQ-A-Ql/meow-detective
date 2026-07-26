use serde::{Deserialize, Serialize};

use super::validation::{DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT};
use crate::paging::validate_opaque_cursor;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetArtifactRowsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_artifact_limit")]
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

impl GetArtifactRowsRequest {
    pub fn validate(&mut self) -> Result<(), String> {
        if self.limit == 0 {
            self.limit = DEFAULT_PAGE_LIMIT;
        }
        self.limit = self.limit.min(MAX_PAGE_LIMIT);
        self.family = self
            .family
            .take()
            .map(|family| family.trim().to_string())
            .filter(|family| !family.is_empty());
        if let Some(cursor) = self.cursor.as_deref() {
            validate_opaque_cursor(cursor).map_err(|error| error.to_string())?;
            if self.offset != 0 {
                return Err("offset must be zero when cursor is provided".to_string());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetArtifactByIdRequest {
    pub artifact_id: String,
}

impl GetArtifactByIdRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.artifact_id.trim().is_empty() {
            return Err("artifactId is required".to_string());
        }
        Ok(())
    }
}

fn default_artifact_limit() -> u32 {
    100
}
