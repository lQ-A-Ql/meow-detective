use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportScopeDto {
    #[serde(default = "default_true")]
    pub file_system_metadata: bool,
    #[serde(default = "default_true")]
    pub registry: bool,
    #[serde(default = "default_true")]
    pub full_timeline: bool,
    #[serde(default)]
    pub raw_file_extraction: bool,
    #[serde(default)]
    pub overwrite: bool,
}

impl Default for ExportScopeDto {
    fn default() -> Self {
        Self {
            file_system_metadata: true,
            registry: true,
            full_timeline: true,
            raw_file_extraction: false,
            overwrite: false,
        }
    }
}

fn default_true() -> bool {
    true
}
