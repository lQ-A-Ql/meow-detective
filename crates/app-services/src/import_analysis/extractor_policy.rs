use std::io::Read;

use artifacts_core::{ExtractorRegistry, VecSink};
use domain::{DataSourcePlatform, FileEntry, FileEntryId};

use super::error::ImportAnalysisError;
use crate::artifact_service::{self, ArtifactExtractionStats, ArtifactServiceError};

/// Platform-owned artifact extractor selection for post-import analysis.
///
/// Linux analysis deliberately has no generic registry here. Its structured
/// extractors run through the Linux analysis service, while shared timeline
/// projection and text indexing remain available in this pipeline.
pub(super) struct PlatformExtractorPolicy {
    registry: Option<ExtractorRegistry>,
}

impl PlatformExtractorPolicy {
    pub(super) fn for_platform(platform: DataSourcePlatform) -> Result<Self, ImportAnalysisError> {
        match platform {
            DataSourcePlatform::Windows => Ok(Self {
                registry: Some(artifact_service::create_registry()),
            }),
            DataSourcePlatform::Linux => Ok(Self { registry: None }),
            DataSourcePlatform::Unknown => Err(unsupported_platform(platform)),
        }
    }

    pub(super) fn should_extract(&self, file: &FileEntry) -> bool {
        self.registry
            .as_ref()
            .is_some_and(|registry| registry_supports_file(registry, file))
    }

    pub(super) fn run_extractors(
        &self,
        file_id: &FileEntryId,
        file_path: &str,
        reader: Box<dyn Read>,
        sink: &mut VecSink,
    ) -> Result<ArtifactExtractionStats, ArtifactServiceError> {
        match self.registry.as_ref() {
            Some(registry) => {
                artifact_service::run_extractors_on_file(registry, file_id, file_path, reader, sink)
            }
            None => Ok(ArtifactExtractionStats::default()),
        }
    }
}

pub(super) fn validate_analysis_platform(
    platform: DataSourcePlatform,
) -> Result<(), ImportAnalysisError> {
    match platform {
        DataSourcePlatform::Windows | DataSourcePlatform::Linux => Ok(()),
        DataSourcePlatform::Unknown => Err(unsupported_platform(platform)),
    }
}

fn unsupported_platform(platform: DataSourcePlatform) -> ImportAnalysisError {
    ImportAnalysisError::UnsupportedPlatform(platform.to_string())
}

pub(super) fn registry_supports_file(registry: &ExtractorRegistry, file: &FileEntry) -> bool {
    !registry.find_for_path(&file.path).is_empty()
        && file
            .size
            .is_some_and(|size| size <= infrastructure::constants::ARTIFACT_FILE_LIMIT_BYTES)
}

#[cfg(test)]
#[path = "../../tests/unit/import_analysis/extractor_policy.rs"]
mod tests;
