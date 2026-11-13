use crate::analysis_service::{android::records, AnalysisServiceError};
use domain::DataSourceId;
use std::path::Path;
use transport::dto::{AnalysisParseStatusDto, AndroidPackageSummaryDto};

pub(crate) fn get_android_package_summary(
    case_root: &Path,
    data_source_id: &DataSourceId,
    offset: u64,
    limit: u32,
) -> Result<AndroidPackageSummaryDto, AnalysisServiceError> {
    let mut packages = Vec::new();
    super::visit_current_kind(case_root, data_source_id, "package", |record| {
        packages.push(records::read_package(record, data_source_id)?);
        Ok(())
    })?;
    packages.sort_by(|a, b| {
        a.package_name
            .to_ascii_lowercase()
            .cmp(&b.package_name.to_ascii_lowercase())
            .then_with(|| a.package_name.cmp(&b.package_name))
    });
    let total_count = packages.len() as u64;
    let packages = packages
        .into_iter()
        .skip(usize::try_from(offset).unwrap_or(usize::MAX))
        .take(limit.clamp(1, 4096) as usize)
        .collect::<Vec<_>>();
    let warnings = packages
        .iter()
        .filter_map(|package| package.warning.clone())
        .collect();
    Ok(AndroidPackageSummaryDto {
        status: if total_count == 0 {
            AnalysisParseStatusDto::NotParsed
        } else {
            AnalysisParseStatusDto::Parsed
        },
        total_count,
        page_total: packages.len() as u64,
        packages,
        warnings,
    })
}
