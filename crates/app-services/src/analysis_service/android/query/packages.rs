use domain::DataSourceId;
use rusqlite::{params, Connection};
use transport::dto::{AnalysisParseStatusDto, AndroidPackageDto, AndroidPackageSummaryDto};

use crate::analysis_service::AnalysisServiceError;

pub(crate) fn get_android_package_summary(
    connection: &Connection,
    data_source_id: &DataSourceId,
    offset: u64,
    limit: u32,
) -> Result<AndroidPackageSummaryDto, AnalysisServiceError> {
    let total_count = connection.query_row(
        "SELECT COUNT(*) FROM android_packages WHERE data_source_id = ?1",
        [data_source_id.0.as_str()],
        |row| row.get(0),
    )?;
    let mut statement = connection.prepare(
        "SELECT package_name, app_name, version_code, install_time, update_time, installer, uid,
                code_path, source_file_id, source_path, parser, warning
         FROM android_packages
         WHERE data_source_id = ?1
         ORDER BY package_name COLLATE NOCASE ASC
         LIMIT ?2 OFFSET ?3",
    )?;
    let packages = statement
        .query_map(params![data_source_id.0.as_str(), limit, offset], |row| {
            Ok(AndroidPackageDto {
                package_name: row.get(0)?,
                app_name: row.get(1)?,
                version_code: row.get(2)?,
                install_time: row.get(3)?,
                update_time: row.get(4)?,
                installer: row.get(5)?,
                uid: row.get(6)?,
                user_id: None,
                user_state: None,
                code_path: row.get(7)?,
                source_file_id: row.get(8)?,
                source_path: row.get(9)?,
                parser: row.get(10)?,
                icon_data_url: None,
                warning: row.get(11)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
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
