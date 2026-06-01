//! Import report generation.
//!
//! Generates detailed reports after import completion.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Import report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportReport {
    /// Data source information
    pub data_source: DataSourceSummary,
    /// Import statistics
    pub statistics: ImportStatistics,
    /// Timeline of events
    pub timeline: Vec<ImportEvent>,
    /// Warnings
    pub warnings: Vec<String>,
    /// Errors
    pub errors: Vec<ImportError>,
    /// Performance metrics
    pub performance: PerformanceMetrics,
}

/// Data source summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceSummary {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub source_path: String,
    pub imported_at: String,
}

/// Import statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportStatistics {
    pub total_files: u64,
    pub total_directories: u64,
    pub total_size: u64,
    pub imported_files: u64,
    pub skipped_files: u64,
    pub error_files: u64,
    pub hash_computed: u64,
    pub artifacts_extracted: u64,
    pub timeline_events: u64,
    pub text_indexed: u64,
}

/// Import event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportEvent {
    pub timestamp: String,
    pub event_type: String,
    pub message: String,
    pub details: Option<String>,
}

/// Import error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportError {
    pub file_path: Option<String>,
    pub error_message: String,
    pub error_type: String,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_duration_ms: u64,
    pub classification_ms: u64,
    pub enumeration_ms: u64,
    pub post_processing_ms: u64,
    pub files_per_second: f64,
    pub bytes_per_second: f64,
    pub peak_memory_bytes: u64,
}

impl ImportReport {
    /// Create a new import report
    pub fn new(data_source: DataSourceSummary) -> Self {
        Self {
            data_source,
            statistics: ImportStatistics::default(),
            timeline: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
            performance: PerformanceMetrics::default(),
        }
    }

    /// Add a timeline event
    pub fn add_event(&mut self, event_type: &str, message: &str) {
        self.timeline.push(ImportEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            event_type: event_type.to_string(),
            message: message.to_string(),
            details: None,
        });
    }

    /// Add a warning
    pub fn add_warning(&mut self, message: &str) {
        self.warnings.push(message.to_string());
    }

    /// Add an error
    pub fn add_error(&mut self, file_path: Option<&str>, error_message: &str) {
        self.errors.push(ImportError {
            file_path: file_path.map(|s| s.to_string()),
            error_message: error_message.to_string(),
            error_type: "import".to_string(),
        });
    }

    /// Update performance metrics
    pub fn update_performance(&mut self, duration: Duration) {
        self.performance.total_duration_ms = duration.as_millis() as u64;

        // Calculate throughput
        if self.performance.total_duration_ms > 0 {
            let seconds = self.performance.total_duration_ms as f64 / 1000.0;
            self.performance.files_per_second = self.statistics.imported_files as f64 / seconds;
            self.performance.bytes_per_second = self.statistics.total_size as f64 / seconds;
        }
    }

    /// Generate summary text
    pub fn summary(&self) -> String {
        format!(
            "Imported {}: {} files, {} dirs, {:.1} MB in {:.1}s ({:.0} files/s)",
            self.data_source.name,
            self.statistics.imported_files,
            self.statistics.total_directories,
            self.statistics.total_size as f64 / (1024.0 * 1024.0),
            self.performance.total_duration_ms as f64 / 1000.0,
            self.performance.files_per_second,
        )
    }

    /// Generate detailed report
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str(&format!("# 导入报告: {}\n\n", self.data_source.name));
        md.push_str(&format!("**数据源**: {}\n", self.data_source.source_path));
        md.push_str(&format!("**类型**: {}\n", self.data_source.kind));
        md.push_str(&format!(
            "**导入时间**: {}\n\n",
            self.data_source.imported_at
        ));

        // Statistics
        md.push_str("## 统计\n\n");
        md.push_str(&format!("- 文件数: {}\n", self.statistics.imported_files));
        md.push_str(&format!(
            "- 目录数: {}\n",
            self.statistics.total_directories
        ));
        md.push_str(&format!(
            "- 总大小: {:.1} MB\n",
            self.statistics.total_size as f64 / (1024.0 * 1024.0)
        ));
        md.push_str(&format!("- 哈希计算: {}\n", self.statistics.hash_computed));
        md.push_str(&format!(
            "- 工件提取: {}\n",
            self.statistics.artifacts_extracted
        ));
        md.push_str(&format!(
            "- 时间线事件: {}\n",
            self.statistics.timeline_events
        ));
        md.push('\n');

        // Performance
        md.push_str("## 性能\n\n");
        md.push_str(&format!(
            "- 总耗时: {:.1}s\n",
            self.performance.total_duration_ms as f64 / 1000.0
        ));
        md.push_str(&format!(
            "- 处理速度: {:.0} 文件/秒\n",
            self.performance.files_per_second
        ));
        md.push_str(&format!(
            "- 吞吐量: {:.1} MB/秒\n",
            self.performance.bytes_per_second / (1024.0 * 1024.0)
        ));
        md.push('\n');

        // Warnings
        if !self.warnings.is_empty() {
            md.push_str("## 警告\n\n");
            for warning in &self.warnings {
                md.push_str(&format!("- {}\n", warning));
            }
            md.push('\n');
        }

        // Errors
        if !self.errors.is_empty() {
            md.push_str("## 错误\n\n");
            for error in &self.errors {
                md.push_str(&format!(
                    "- {}: {}\n",
                    error.file_path.as_deref().unwrap_or("(unknown)"),
                    error.error_message
                ));
            }
        }

        md
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            total_duration_ms: 0,
            classification_ms: 0,
            enumeration_ms: 0,
            post_processing_ms: 0,
            files_per_second: 0.0,
            bytes_per_second: 0.0,
            peak_memory_bytes: 0,
        }
    }
}

#[cfg(test)]
mod tests {
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
}
