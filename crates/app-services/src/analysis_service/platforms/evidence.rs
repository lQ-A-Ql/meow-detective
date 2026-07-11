use domain::DataSourcePlatform;

use super::{analyzer::analyzer_for, windows::WINDOWS_EVIDENCE_CATEGORIES};
use crate::analysis_service::capability::{
    ensure_platform_match, find_capability, reject_retired_or_blank_key, LINUX_UMBRELLA_KEY,
};
use crate::analysis_service::error::AnalysisServiceError;

const SHARED_EVIDENCE_CATEGORIES: &[&str] = &["FileTypeInventory"];

pub(crate) fn evidence_summary_category_allowed(
    platform: DataSourcePlatform,
    category: &str,
) -> Result<bool, AnalysisServiceError> {
    analyzer_for(platform)?;
    Ok(SHARED_EVIDENCE_CATEGORIES.contains(&category)
        || match platform {
            DataSourcePlatform::Windows => WINDOWS_EVIDENCE_CATEGORIES.contains(&category),
            DataSourcePlatform::Linux => category == LINUX_UMBRELLA_KEY,
            DataSourcePlatform::Unknown => false,
        })
}

pub fn select_evidence_scan_categories<'a>(
    platform: DataSourcePlatform,
    requested: &[&'a str],
) -> Result<Vec<&'a str>, AnalysisServiceError> {
    if platform != DataSourcePlatform::Windows {
        analyzer_for(platform)?;
        return Err(AnalysisServiceError::Unsupported(
            "targeted evidence classification is Windows-only; use run_analysis_extraction for Linux"
                .to_string(),
        ));
    }
    let analyzer = analyzer_for(platform)?;
    if requested.is_empty() {
        return Ok(analyzer.default_evidence_categories().to_vec());
    }

    let mut selected = Vec::new();
    for raw_key in requested {
        let key = raw_key.trim();
        reject_retired_or_blank_key(key)?;
        let candidate_category = evidence_category_for_key(platform, key)?;
        if !selected.contains(&candidate_category) {
            selected.push(candidate_category);
        }
    }
    Ok(selected)
}

fn evidence_category_for_key(
    platform: DataSourcePlatform,
    key: &str,
) -> Result<&str, AnalysisServiceError> {
    if SHARED_EVIDENCE_CATEGORIES.contains(&key) {
        return Ok(key);
    }
    if key == LINUX_UMBRELLA_KEY {
        ensure_platform_match(key, platform, DataSourcePlatform::Linux)?;
        return Ok(LINUX_UMBRELLA_KEY);
    }
    if let Some(capability) = find_capability(key) {
        ensure_platform_match(key, platform, capability.platform)?;
        return Ok(capability.candidate_category);
    }
    if WINDOWS_EVIDENCE_CATEGORIES.contains(&key) {
        ensure_platform_match(key, platform, DataSourcePlatform::Windows)?;
        return Ok(key);
    }
    Err(AnalysisServiceError::InvalidInput(format!(
        "unknown evidence analysis category `{key}`"
    )))
}
