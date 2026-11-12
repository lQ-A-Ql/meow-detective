use domain::{DataSourcePlatform, FileEntry};

use super::error::ImportAnalysisError;

/// Platform-owned artifact extractor selection for post-import analysis.
///
/// Platform-specific artifact extraction runs after Catalog publication through
/// the source analysis services, so the generic import workers only keep
/// metadata indexing and shared timeline work.
pub(super) struct PlatformExtractorPolicy;

impl PlatformExtractorPolicy {
    pub(super) fn for_platform(platform: DataSourcePlatform) -> Result<Self, ImportAnalysisError> {
        match platform {
            DataSourcePlatform::Windows => Ok(Self),
            DataSourcePlatform::Linux => Ok(Self),
            DataSourcePlatform::Android => Ok(Self),
            DataSourcePlatform::Unknown => Err(unsupported_platform(platform)),
        }
    }

    pub(super) fn should_extract(&self, _file: &FileEntry) -> bool {
        false
    }
}

pub(super) fn validate_analysis_platform(
    platform: DataSourcePlatform,
) -> Result<(), ImportAnalysisError> {
    match platform {
        DataSourcePlatform::Windows | DataSourcePlatform::Linux | DataSourcePlatform::Android => {
            Ok(())
        }
        DataSourcePlatform::Unknown => Err(unsupported_platform(platform)),
    }
}

fn unsupported_platform(platform: DataSourcePlatform) -> ImportAnalysisError {
    ImportAnalysisError::UnsupportedPlatform(platform.to_string())
}

#[cfg(test)]
pub(super) fn registry_supports_file(
    registry: &artifacts_core::ExtractorRegistry,
    file: &FileEntry,
) -> bool {
    !file.encrypted
        && !registry.find_for_path(&file.path).is_empty()
        && file
            .size
            .is_some_and(|size| size <= infrastructure::constants::ARTIFACT_FILE_LIMIT_BYTES)
}

#[cfg(test)]
#[path = "../../tests/unit/import_analysis/extractor_policy.rs"]
mod tests;
