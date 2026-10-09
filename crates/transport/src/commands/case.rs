use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCaseRequest {
    pub case_root: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examiner: Option<String>,
}

impl CreateCaseRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.case_root.trim().is_empty() {
            return Err("caseRoot is required".to_string());
        }
        if self.name.trim().is_empty() {
            return Err("name is required".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCaseRequest {
    pub case_root: String,
}

impl OpenCaseRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.case_root.trim().is_empty() {
            return Err("caseRoot is required".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameDataSourceRequest {
    pub data_source_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetNestedEvidenceLineageRequest {
    pub parent_data_source_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterializeNestedEvidenceRequest {
    pub parent_data_source_id: String,
    pub file_entry_id: String,
}

impl MaterializeNestedEvidenceRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.parent_data_source_id.trim().is_empty() {
            return Err("parentDataSourceId is required".to_string());
        }
        if self.file_entry_id.trim().is_empty() {
            return Err("fileEntryId is required".to_string());
        }
        Ok(())
    }
}

impl GetNestedEvidenceLineageRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.parent_data_source_id.trim().is_empty() {
            Err("parentDataSourceId is required".to_string())
        } else {
            Ok(())
        }
    }
}

impl RenameDataSourceRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.data_source_id.trim().is_empty() {
            return Err("dataSourceId is required".to_string());
        }
        if self.name.trim().is_empty() {
            return Err("name is required".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCaseRequest {
    pub case_root: String,
}

impl DeleteCaseRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.case_root.trim().is_empty() {
            return Err("caseRoot is required".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteDataSourceRequest {
    pub data_source_id: String,
}

impl DeleteDataSourceRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.data_source_id.trim().is_empty() {
            return Err("dataSourceId is required".to_string());
        }
        Ok(())
    }
}
