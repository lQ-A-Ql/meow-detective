use serde::{Deserialize, Serialize};

use super::{validation::validate_import_source_path, ImportTargetPlatformDto};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDataSourceRequest {
    pub source_path: String,
    #[serde(default, skip_serializing_if = "is_default_import_source_kind")]
    pub source_kind: ImportSourceKindDto,
    pub platform: ImportTargetPlatformDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

impl ImportDataSourceRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_import_source_path(&self.source_path)?;
        if let Some(profile) = &self.profile {
            if profile.trim().is_empty() {
                return Err("profile must not be empty when provided".to_string());
            }
            if profile.contains('\0') {
                return Err("profile contains a null byte".to_string());
            }
        }
        if self.source_kind == ImportSourceKindDto::LinuxCluster
            && self.platform != ImportTargetPlatformDto::Linux
        {
            return Err("linuxCluster imports must use platform linux".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum ImportSourceKindDto {
    #[default]
    Auto,
    LinuxCluster,
}

fn is_default_import_source_kind(value: &ImportSourceKindDto) -> bool {
    *value == ImportSourceKindDto::Auto
}
