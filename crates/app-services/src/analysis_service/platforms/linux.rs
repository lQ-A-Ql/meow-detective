use domain::DataSourcePlatform;

use super::PlatformAnalyzer;
use crate::analysis_service::capability::{AnalysisCapability, LINUX_CAPABILITIES};

pub(super) static LINUX_ANALYZER: LinuxAnalyzer = LinuxAnalyzer;

pub(super) struct LinuxAnalyzer;

impl PlatformAnalyzer for LinuxAnalyzer {
    fn platform(&self) -> DataSourcePlatform {
        DataSourcePlatform::Linux
    }

    fn capabilities(&self) -> &'static [AnalysisCapability] {
        LINUX_CAPABILITIES
    }

    fn default_evidence_categories(&self) -> &'static [&'static str] {
        // Targeted evidence classification (`select_evidence_scan_categories`
        // in platforms/evidence.rs) is Windows-only — Linux sources always run
        // the full `run_analysis_extraction` umbrella, so no default category
        // list is ever consulted for this platform.
        &[]
    }
}
