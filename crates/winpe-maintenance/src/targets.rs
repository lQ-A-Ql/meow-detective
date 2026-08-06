//! Reads the host-generated TARGETS.JSON manifest from the maintenance
//! CD-ROM so the in-guest run can cross-check host-side preflight knowledge
//! against what the guest actually sees.

use std::path::PathBuf;

use serde::Deserialize;

use crate::runtime::windows_drive_roots;
use crate::MaintenanceError;

const TARGETS_FILE: &str = "TARGETS.JSON";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceTargets {
    #[serde(default)]
    pub data_source_id: String,
    #[serde(default)]
    pub installs: Vec<InstallTarget>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallTarget {
    pub partition_index: u32,
    #[serde(default)]
    pub osdata_present: bool,
    #[serde(default)]
    pub utilman_bypass_available: bool,
}

pub fn load_targets() -> Result<Option<(PathBuf, MaintenanceTargets)>, MaintenanceError> {
    for root in windows_drive_roots() {
        let candidate = root.join(TARGETS_FILE);
        if !candidate.is_file() {
            continue;
        }
        let bytes = std::fs::read(&candidate)?;
        let targets = serde_json::from_slice(&bytes)
            .map_err(|error| MaintenanceError::InvalidTargets(error.to_string()))?;
        return Ok(Some((candidate, targets)));
    }
    Ok(None)
}
