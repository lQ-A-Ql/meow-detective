use domain::DataSourcePlatform;

use super::PlatformAnalyzer;
use crate::analysis_service::capability::{AnalysisCapability, WINDOWS_CAPABILITIES};

pub(super) static WINDOWS_ANALYZER: WindowsAnalyzer = WindowsAnalyzer;
pub(super) const WINDOWS_EVIDENCE_CATEGORIES: &[&str] = &[
    "SystemInformation",
    "Registry",
    "EventLogs",
    "ProgramExecution",
    "UserActivity",
    "RecycleBin",
    "Thumbnails",
    "ResourceUsage",
    "BrowserHistory",
    "Email",
];
const WINDOWS_EVIDENCE_DEFAULTS: &[&str] = &[
    "SystemInformation",
    "Registry",
    "ProgramExecution",
    "UserActivity",
    "RecycleBin",
    "Thumbnails",
    "ResourceUsage",
    "BrowserHistory",
    "Email",
];

pub(super) struct WindowsAnalyzer;

impl PlatformAnalyzer for WindowsAnalyzer {
    fn platform(&self) -> DataSourcePlatform {
        DataSourcePlatform::Windows
    }

    fn capabilities(&self) -> &'static [AnalysisCapability] {
        WINDOWS_CAPABILITIES
    }

    fn default_evidence_categories(&self) -> &'static [&'static str] {
        WINDOWS_EVIDENCE_DEFAULTS
    }
}
