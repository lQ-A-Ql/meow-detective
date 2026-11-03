use domain::DataSourceId;
use persistence_fjall::ScanDirection;
use persistence_sqlite::repositories::android_analysis_repo::AndroidAnalysisRepo;
use rusqlite::Connection;
use std::path::Path;
use transport::dto::{AnalysisParseStatusDto, AndroidPackageDto, AndroidPackageSummaryDto};

use crate::analysis_service::AnalysisServiceError;

pub(crate) fn get_android_package_summary(
    connection: &Connection,
    case_root: &Path,
    data_source_id: &DataSourceId,
    offset: u64,
    limit: u32,
) -> Result<AndroidPackageSummaryDto, AnalysisServiceError> {
    if let Ok(source) =
        crate::source_db::open_registered_fjall_source_read_only(case_root, data_source_id)
    {
        let snapshot = source.snapshot();
        let complete = source
            .source_projection_complete(&snapshot, &data_source_id.0)
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
        if complete {
            if let Some(result) = read_fjall_packages(&source, data_source_id, offset, limit)? {
                return Ok(result);
            }
        }
    }
    read_sqlite_packages(connection, data_source_id, offset, limit)
}

fn read_fjall_packages(
    source: &crate::source_db::FjallReadOnlySourceStorageHandle,
    data_source_id: &DataSourceId,
    offset: u64,
    limit: u32,
) -> Result<Option<AndroidPackageSummaryDto>, AnalysisServiceError> {
    let snapshot = source.snapshot();
    let page_size = usize::try_from(limit).unwrap_or(usize::MAX).clamp(1, 4096);
    let mut cursor = None;
    let mut skipped = 0u64;
    let mut packages = Vec::new();
    let mut total_count = 0u64;
    loop {
        let page = source
            .parser_list_by_kind(
                &snapshot,
                &data_source_id.0,
                "package",
                cursor.as_deref(),
                ScanDirection::Forward,
                page_size,
            )
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
        if page.records.is_empty() {
            break;
        }
        total_count = total_count.saturating_add(page.records.len() as u64);
        for record in page.records {
            if skipped < offset {
                skipped = skipped.saturating_add(1);
                continue;
            }
            if packages.len() >= page_size {
                continue;
            }
            let value = record.payload;
            if let Some(package) = package_from_payload(value) {
                packages.push(package);
            }
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    if total_count == 0 {
        return Ok(None);
    }
    let warnings = packages
        .iter()
        .filter_map(|package| package.warning.clone())
        .collect();
    Ok(Some(AndroidPackageSummaryDto {
        status: AnalysisParseStatusDto::Parsed,
        total_count,
        page_total: packages.len() as u64,
        packages,
        warnings,
    }))
}

fn package_from_payload(value: serde_json::Value) -> Option<AndroidPackageDto> {
    Some(AndroidPackageDto {
        package_name: value.get("packageName")?.as_str()?.to_owned(),
        app_name: value
            .get("appName")
            .and_then(|value| value.as_str().map(str::to_owned)),
        version_code: value
            .get("versionCode")
            .and_then(|value| value.as_str().map(str::to_owned)),
        install_time: value
            .get("installTime")
            .and_then(|value| value.as_str().map(str::to_owned)),
        update_time: value
            .get("updateTime")
            .and_then(|value| value.as_str().map(str::to_owned)),
        installer: value
            .get("installer")
            .and_then(|value| value.as_str().map(str::to_owned)),
        uid: value
            .get("uid")
            .and_then(|value| value.as_u64())
            .map(|value| value as u32),
        user_id: None,
        user_state: None,
        code_path: value
            .get("codePath")
            .and_then(|value| value.as_str().map(str::to_owned)),
        source_file_id: value.get("sourceFileId")?.as_str()?.to_owned(),
        source_path: value.get("sourcePath")?.as_str()?.to_owned(),
        parser: value.get("parser")?.as_str()?.to_owned(),
        icon_data_url: None,
        warning: value
            .get("warning")
            .and_then(|value| value.as_str().map(str::to_owned)),
    })
}

fn read_sqlite_packages(
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
