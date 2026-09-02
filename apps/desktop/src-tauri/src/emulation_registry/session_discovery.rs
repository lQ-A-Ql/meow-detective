//! Durable discovery for emulation sessions owned by another app process.

use std::fs;
use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

use super::vmware::{self, VmwareError};

const PROVENANCE_FILE: &str = "provenance.json";
const VMX_FILE: &str = "machine.vmx";

#[derive(Debug, Error)]
pub(super) enum SessionDiscoveryError {
    #[error("emulation session workspace inspection failed")]
    Io,
    #[error("VMware session query failed: {0}")]
    Vmware(String),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProvenanceRecord {
    session_id: String,
    data_source_id: String,
}

/// Finds a VM belonging to `data_source_id` that is currently listed by
/// VMware. Stale workspaces are deliberately ignored and left for explicit
/// release/cleanup; discovery must not affect another investigator's VM.
pub(super) fn find_active_session(
    case_root: &Path,
    data_source_id: &str,
) -> Result<Option<String>, SessionDiscoveryError> {
    find_active_session_with(
        case_root,
        data_source_id,
        |vmx| match vmware::is_vmx_running(vmx) {
            Ok(running) => Ok(running),
            Err(VmwareError::NotInstalled) => Ok(false),
            Err(error) => Err(SessionDiscoveryError::Vmware(error.to_string())),
        },
    )
}

pub(super) fn find_active_session_with<F>(
    case_root: &Path,
    data_source_id: &str,
    mut is_running: F,
) -> Result<Option<String>, SessionDiscoveryError>
where
    F: FnMut(&Path) -> Result<bool, SessionDiscoveryError>,
{
    let emulation_root = case_root.join("emulation");
    let entries = match fs::read_dir(&emulation_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            tracing::error!(error = %error, "failed to inspect emulation sessions");
            return Err(SessionDiscoveryError::Io);
        }
    };

    for entry in entries {
        let entry = entry.map_err(|error| {
            tracing::error!(error = %error, "failed to inspect emulation session");
            SessionDiscoveryError::Io
        })?;
        let root = entry.path();
        if !is_regular_directory(&root) {
            continue;
        }
        let Some(record) = read_provenance(&root)? else {
            continue;
        };
        if record.data_source_id != data_source_id || !valid_session_id(&record.session_id) {
            continue;
        }
        let vmx = root.join(VMX_FILE);
        if !vmx.is_file() {
            continue;
        }
        if is_running(&vmx)? {
            return Ok(Some(record.session_id));
        }
    }
    Ok(None)
}

fn read_provenance(root: &Path) -> Result<Option<ProvenanceRecord>, SessionDiscoveryError> {
    let path = root.join(PROVENANCE_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            tracing::error!(error = %error, "failed to inspect emulation provenance");
            return Err(SessionDiscoveryError::Io);
        }
    };
    if !metadata.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(|error| {
        tracing::error!(error = %error, "failed to read emulation provenance");
        SessionDiscoveryError::Io
    })?;
    match serde_json::from_slice(&bytes) {
        Ok(record) => Ok(Some(record)),
        Err(error) => {
            tracing::warn!(error = %error, "ignoring malformed emulation provenance");
            Ok(None)
        }
    }
}

fn is_regular_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
}

fn valid_session_id(session_id: &str) -> bool {
    let Some(uuid) = session_id.strip_prefix("emulation-") else {
        return false;
    };
    uuid::Uuid::parse_str(uuid).is_ok()
}

#[cfg(test)]
#[path = "../../tests/unit/emulation_registry/session_discovery.rs"]
mod tests;
