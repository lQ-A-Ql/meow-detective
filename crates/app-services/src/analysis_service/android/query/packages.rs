use domain::DataSourceId;
use persistence_sqlite::repositories::android_analysis_repo::AndroidAnalysisRepo;
use rusqlite::Connection;
use transport::dto::{AnalysisParseStatusDto, AndroidPackageDto, AndroidPackageSummaryDto};

use crate::analysis_service::AnalysisServiceError;

pub(crate) fn get_android_package_summary(
    connection: &Connection,
    data_source_id: &DataSourceId,
    offset: u64,
    limit: u32,
) -> Result<AndroidPackageSummaryDto, AnalysisServiceError> {
    let repository = AndroidAnalysisRepo::new(connection);
    let total_count = repository.count_packages(data_source_id)?;
    let packages = repository
        .list_packages_page(data_source_id, offset, limit)?
        .into_iter()
        .map(|package| AndroidPackageDto {
            package_name: package.package_name,
            app_name: package.app_name,
            version_code: package.version_code,
            install_time: package.install_time,
            update_time: package.update_time,
            installer: package.installer,
            uid: package.uid,
            user_id: None,
            user_state: None,
            code_path: package.code_path,
            source_file_id: package.source_file_id,
            source_path: package.source_path,
            parser: package.parser,
            icon_data_url: None,
            warning: package.warning,
        })
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
