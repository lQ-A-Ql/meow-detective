use std::path::PathBuf;

use app_services::analysis_service::{
    get_source_android_device_info, get_source_android_package_summary,
    run_source_android_analysis, AnalysisSourceReadRuntime,
};
use domain::{CaseId, DataSourceId};

#[test]
#[ignore = "requires an explicitly selected private Android source database"]
fn private_android_source_produces_package_metadata() {
    let case_root = required_path("FORENSICS_ANDROID_CASE_ROOT");
    let case_id = CaseId(required_value("FORENSICS_ANDROID_CASE_ID"));
    let data_source_id = DataSourceId(required_value("FORENSICS_ANDROID_SOURCE_ID"));
    let case_connection =
        persistence_sqlite::open_existing(&case_root.join("app.db")).expect("open case database");
    let runtime = AnalysisSourceReadRuntime::default();

    let run = run_source_android_analysis(
        &case_connection,
        &case_root,
        &case_id,
        &data_source_id,
        &runtime,
    )
    .expect("run Android analysis");
    let device =
        get_source_android_device_info(&case_connection, &case_root, &case_id, &data_source_id)
            .expect("read Android device facts");
    let packages = get_source_android_package_summary(
        &case_connection,
        &case_root,
        &case_id,
        &data_source_id,
        0,
        20,
        &runtime,
    )
    .expect("read Android package summary");
    let user_state_count = packages
        .packages
        .iter()
        .filter(|package| package.user_state.is_some())
        .count();
    let presentation_count = packages
        .packages
        .iter()
        .filter(|package| package.app_name.is_some() || package.icon_data_url.is_some())
        .count();
    let package_warning_count = packages
        .packages
        .iter()
        .filter(|package| package.warning.is_some())
        .count();
    let first_package_warning = packages
        .packages
        .iter()
        .find_map(|package| package.warning.as_deref());
    let package_diagnostics = packages
        .packages
        .iter()
        .map(|package| {
            format!(
                "{}:{}:{}:{}",
                package.package_name,
                package.app_name.is_some(),
                package.icon_data_url.is_some(),
                package.warning.is_some(),
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    println!(
        "android analysis: scanned_files={}, device_facts={}, packages={}, first_page={}, user_states={}, presentations={}, package_warnings={}, first_package_warning={:?}, device_status={:?}",
        run.scanned_file_count,
        run.device_fact_count,
        packages.total_count,
        packages.page_total,
        user_state_count,
        presentation_count,
        package_warning_count,
        first_package_warning,
        device.status,
    );
    println!("package presentations: {package_diagnostics}");
    assert!(
        packages.total_count > 0,
        "private source must expose package metadata"
    );
}

fn required_path(name: &str) -> PathBuf {
    PathBuf::from(required_value(name))
}

fn required_value(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name} for this ignored test"))
}
