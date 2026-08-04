use std::path::Path;

use domain::{DataSourceId, DataSourceKind};

use super::MountServiceError;

pub(super) fn mount_source_binding(
    data_source_id: &DataSourceId,
    source_hash: Option<String>,
) -> String {
    source_hash
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("data-source-id:{}", data_source_id.0))
}

pub(super) fn validate_source_kind(source_kind: &DataSourceKind) -> Result<(), MountServiceError> {
    if matches!(source_kind, DataSourceKind::E01 | DataSourceKind::Raw) {
        return Ok(());
    }
    Err(MountServiceError::Unsupported(format!(
        "data source kind '{source_kind}' is outside the E01/raw mount boundary"
    )))
}

pub(super) fn validate_source_identity(
    source_path: &Path,
    expected_size: Option<u64>,
) -> Result<(), MountServiceError> {
    let Some(expected) = expected_size else {
        return Ok(());
    };
    let actual = std::fs::metadata(source_path)?.len();
    if actual != expected {
        return Err(MountServiceError::SourceIdentityMismatch { expected, actual });
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/mount_service/source_validation.rs"]
mod tests;
