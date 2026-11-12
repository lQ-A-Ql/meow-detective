use domain::DataSourcePlatform;

use super::PlatformAnalyzer;
use crate::analysis_service::capability::{AnalysisCapability, ANDROID_CAPABILITIES};

pub(super) static ANDROID_ANALYZER: AndroidAnalyzer = AndroidAnalyzer;

pub(super) struct AndroidAnalyzer;

impl PlatformAnalyzer for AndroidAnalyzer {
    fn platform(&self) -> DataSourcePlatform {
        DataSourcePlatform::Android
    }

    fn capabilities(&self) -> &'static [AnalysisCapability] {
        ANDROID_CAPABILITIES
    }

    fn default_evidence_categories(&self) -> &'static [&'static str] {
        &[]
    }
}
