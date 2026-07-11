use domain::DataSourcePlatform;

use super::{linux, windows};
use crate::analysis_service::capability::{select_capabilities, AnalysisCapability};
use crate::analysis_service::error::AnalysisServiceError;

pub(crate) trait PlatformAnalyzer: Sync {
    fn platform(&self) -> DataSourcePlatform;
    fn capabilities(&self) -> &'static [AnalysisCapability];
    fn default_evidence_categories(&self) -> &'static [&'static str];

    fn select_capabilities(
        &self,
        requested: &[&str],
    ) -> Result<Vec<AnalysisCapability>, AnalysisServiceError> {
        select_capabilities(self.platform(), self.capabilities(), requested)
    }
}

pub(crate) fn analyzer_for(
    platform: DataSourcePlatform,
) -> Result<&'static dyn PlatformAnalyzer, AnalysisServiceError> {
    match platform {
        DataSourcePlatform::Windows => Ok(&windows::WINDOWS_ANALYZER),
        DataSourcePlatform::Linux => Ok(&linux::LINUX_ANALYZER),
        DataSourcePlatform::Unknown => Err(AnalysisServiceError::unsupported_platform(
            "platform metadata is missing",
        )),
    }
}

pub fn validate_analysis_categories(
    platform: DataSourcePlatform,
    requested: &[&str],
) -> Result<(), AnalysisServiceError> {
    analyzer_for(platform)?.select_capabilities(requested)?;
    Ok(())
}
