use super::*;

fn create_test_report() -> ImportReport {
    ImportReport::new(DataSourceSummary {
        id: "ds-1".to_string(),
        name: "Test Source".to_string(),
        kind: "E01".to_string(),
        source_path: "/path/to/image.E01".to_string(),
        imported_at: "2026-05-31T00:00:00Z".to_string(),
    })
}

#[test]
fn test_report_creation() {
    let report = create_test_report();
    assert_eq!(report.data_source.name, "Test Source");
    assert!(report.warnings.is_empty());
    assert!(report.errors.is_empty());
}

#[test]
fn test_report_add_event() {
    let mut report = create_test_report();
    report.add_event("info", "Import started");
    assert_eq!(report.timeline.len(), 1);
}

#[test]
fn test_report_add_warning() {
    let mut report = create_test_report();
    report.add_warning("Test warning");
    assert_eq!(report.warnings.len(), 1);
}

#[test]
fn test_report_add_error() {
    let mut report = create_test_report();
    report.add_error(Some("/path"), "Test error");
    assert_eq!(report.errors.len(), 1);
}

#[test]
fn test_report_summary() {
    let mut report = create_test_report();
    report.statistics.imported_files = 100;
    report.statistics.total_directories = 10;
    report.statistics.total_size = 1024 * 1024;
    report.performance.total_duration_ms = 1000;
    report.performance.files_per_second = 100.0;

    let summary = report.summary();
    assert!(summary.contains("100 files"));
    assert!(summary.contains("10 dirs"));
}

#[test]
fn test_report_markdown() {
    let mut report = create_test_report();
    report.statistics.imported_files = 100;
    report.add_warning("Test warning");

    let md = report.to_markdown();
    assert!(md.contains("# 导入报告"));
    assert!(md.contains("Test warning"));
}

#[test]
fn test_performance_calculation() {
    let mut report = create_test_report();
    report.statistics.imported_files = 1000;
    report.statistics.total_size = 1024 * 1024 * 100;

    report.update_performance(Duration::from_secs(10));
    assert_eq!(report.performance.total_duration_ms, 10000);
    assert!(report.performance.files_per_second > 0.0);
}
