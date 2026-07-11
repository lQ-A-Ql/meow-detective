use domain::DataSourcePlatform;

use super::PlatformAnalyzer;
use crate::analysis_service::capability::{AnalysisCapability, LINUX_CAPABILITIES};

pub(super) static LINUX_ANALYZER: LinuxAnalyzer = LinuxAnalyzer;
const LINUX_EVIDENCE_DEFAULTS: &[&str] = &["LinuxArtifacts"];

pub(super) struct LinuxAnalyzer;

impl PlatformAnalyzer for LinuxAnalyzer {
    fn platform(&self) -> DataSourcePlatform {
        DataSourcePlatform::Linux
    }

    fn capabilities(&self) -> &'static [AnalysisCapability] {
        LINUX_CAPABILITIES
    }

    fn default_evidence_categories(&self) -> &'static [&'static str] {
        LINUX_EVIDENCE_DEFAULTS
    }
}
