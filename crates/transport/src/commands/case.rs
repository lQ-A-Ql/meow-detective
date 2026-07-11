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
