use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MountImageRequestDto {
    pub data_source_id: String,
    pub partition_index: u32,
    #[serde(default)]
    pub mount_point: Option<String>,
}

impl MountImageRequestDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.data_source_id.trim().is_empty() {
            return Err("data source id is required");
        }
        if let Some(mount_point) = &self.mount_point {
            if mount_point.trim().is_empty() {
                return Err("mount point cannot be empty");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MountTargetDto {
    pub mount_id: String,
    pub data_source_id: String,
    pub partition_index: u32,
    pub filesystem: String,
    pub mount_point: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MountStateDto {
    Preparing,
    Mounted,
    Unmounting,
    Released,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MountStatusDto {
    pub target: MountTargetDto,
    pub state: MountStateDto,
    pub active_handle_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/mount.rs"]
mod tests;
