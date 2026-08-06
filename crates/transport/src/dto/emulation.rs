use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationOptionsDto {
    #[serde(default)]
    pub network: bool,
    #[serde(default)]
    pub clipboard: bool,
    #[serde(default)]
    pub time_sync: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareEmulationRequestDto {
    pub data_source_id: String,
    #[serde(default)]
    pub recovery_iso_path: Option<String>,
    #[serde(default)]
    pub allow_direct_boot: bool,
    #[serde(default)]
    pub options: EmulationOptionsDto,
}

impl PrepareEmulationRequestDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.data_source_id.trim().is_empty() {
            return Err("data source id is required");
        }
        if self
            .recovery_iso_path
            .as_deref()
            .is_some_and(|path| path.trim().is_empty())
        {
            return Err("recovery ISO path cannot be empty");
        }
        match (self.recovery_iso_path.is_some(), self.allow_direct_boot) {
            (false, false) => {
                return Err("direct system boot requires explicit confirmation");
            }
            (true, true) => {
                return Err("direct boot confirmation cannot be combined with recovery media");
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EmulationStateDto {
    DescriptorReady,
    Running,
    Quiescing,
    Released,
    FailedCleanupPending,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EmulationControlModeDto {
    InteractiveOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EmulationBootRouteDto {
    RecoveryMedia,
    DirectSystem,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationInstallDto {
    pub partition_index: u32,
    pub osdata_present: bool,
    pub sam_present: bool,
    pub utilman_bypass_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationPreflightDto {
    pub data_source_id: String,
    pub installs: Vec<EmulationInstallDto>,
    pub recommended_boot_route: EmulationBootRouteDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmulationSessionStatusDto {
    pub session_id: String,
    pub data_source_id: String,
    pub state: EmulationStateDto,
    pub logical_length: u64,
    pub control_mode: EmulationControlModeDto,
    pub error: Option<String>,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/emulation.rs"]
mod tests;
