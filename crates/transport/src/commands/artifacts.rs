use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetArtifactRowsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
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
