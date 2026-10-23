use std::path::Path;
use std::sync::atomic::AtomicBool;

use domain::{CaseId, DataSourceId};
use rusqlite::Connection;
use transport::dto::{AndroidAnalysisRunDto, AndroidDeviceInfoDto, AndroidPackageSummaryDto};

use super::runtime::AnalysisSourceReadRuntime;
use super::source::{open_ready_analysis_source, open_ready_analysis_source_read_only};
use crate::analysis_service::android;
use crate::analysis_service::{validate_analysis_categories, AnalysisServiceError};
use crate::file_service::SourceReadContext;

pub fn run_source_android_analysis(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    runtime: &AnalysisSourceReadRuntime,
) -> Result<AndroidAnalysisRunDto, AnalysisServiceError> {
    run_source_android_analysis_with_cancel(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        runtime,
        &AtomicBool::new(false),
    )
}

pub fn run_source_android_analysis_with_cancel(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    runtime: &AnalysisSourceReadRuntime,
    cancel_token: &AtomicBool,
) -> Result<AndroidAnalysisRunDto, AnalysisServiceError> {
    super::super::cancellation::ensure_not_cancelled(cancel_token)?;
    let source = open_ready_analysis_source(case_conn, case_root, case_id, data_source_id)?;
    validate_analysis_categories(source.platform, &["AndroidDevice", "AndroidPackages"])?;
    let mut reader = runtime.bind(SourceReadContext::new(
        &source.connection,
        case_conn,
        case_root,
        case_id,
        data_source_id,
    ));
    let result =
        android::run_android_source_analysis(&source.connection, &mut reader, data_source_id)?;
    super::super::cancellation::ensure_not_cancelled(cancel_token)?;
    Ok(result)
}

pub fn get_source_android_device_info(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<AndroidDeviceInfoDto, AnalysisServiceError> {
    let source =
        open_ready_analysis_source_read_only(case_conn, case_root, case_id, data_source_id)?;
    validate_analysis_categories(source.platform, &["AndroidDevice"])?;
    android::get_android_device_info(&source.connection, data_source_id)
}

pub fn get_source_android_package_summary(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    offset: u64,
    limit: u32,
    runtime: &AnalysisSourceReadRuntime,
) -> Result<AndroidPackageSummaryDto, AnalysisServiceError> {
    let source =
        open_ready_analysis_source_read_only(case_conn, case_root, case_id, data_source_id)?;
    validate_analysis_categories(source.platform, &["AndroidPackages"])?;
    let mut reader = runtime.bind(SourceReadContext::new(
        &source.connection,
        case_conn,
        case_root,
        case_id,
        data_source_id,
    ));
    let mut summary =
        android::get_android_package_summary(&source.connection, data_source_id, offset, limit)?;
    android::hydrate_package_presentations(&source.connection, &mut reader, &mut summary)?;
    Ok(summary)
}
