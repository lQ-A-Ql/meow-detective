use serde::{Deserialize, Serialize};

use super::validation::{validate_required_data_source_id, MAX_PAGE_LIMIT};
use crate::dto::EvtxEventViewDto;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifyFilesRequest {
    pub data_source_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_size: Option<u32>,
}

impl ClassifyFilesRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_required_data_source_id(&self.data_source_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RunEvidenceClassificationRequest {
    pub data_source_id: String,
    #[serde(default)]
    pub categories: Vec<String>,
}

impl RunEvidenceClassificationRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_required_data_source_id(&self.data_source_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RunAnalysisExtractionRequest {
    pub data_source_id: String,
    #[serde(default)]
    pub categories: Vec<String>,
}

impl RunAnalysisExtractionRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_required_data_source_id(&self.data_source_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAnalysisSourceRequest {
    pub data_source_id: String,
}

impl GetAnalysisSourceRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_required_data_source_id(&self.data_source_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAnalysisExtractionRequest {
    pub data_source_id: String,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_analysis_extraction_limit")]
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetEvtxEventSummaryRequest {
    pub data_source_id: String,
    #[serde(default)]
    pub view: Option<EvtxEventViewDto>,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_analysis_extraction_limit")]
    pub limit: u32,
}

impl Default for GetEvtxEventSummaryRequest {
    fn default() -> Self {
        Self {
            data_source_id: String::new(),
            view: None,
            offset: 0,
            limit: default_analysis_extraction_limit(),
        }
    }
}

impl GetEvtxEventSummaryRequest {
    pub fn validate(&mut self) -> Result<(), String> {
        validate_required_data_source_id(&self.data_source_id)?;
        if self.limit == 0 {
            self.limit = default_analysis_extraction_limit();
        }
        self.limit = self.limit.min(MAX_PAGE_LIMIT);
        Ok(())
    }
}

impl Default for GetAnalysisExtractionRequest {
    fn default() -> Self {
        Self {
            data_source_id: String::new(),
            offset: 0,
            limit: default_analysis_extraction_limit(),
        }
    }
}

impl GetAnalysisExtractionRequest {
    pub fn validate(&mut self) -> Result<(), String> {
        validate_required_data_source_id(&self.data_source_id)?;
        if self.limit == 0 {
            self.limit = default_analysis_extraction_limit();
        }
        self.limit = self.limit.min(MAX_PAGE_LIMIT);
        Ok(())
    }
}

fn default_analysis_extraction_limit() -> u32 {
    100
}
