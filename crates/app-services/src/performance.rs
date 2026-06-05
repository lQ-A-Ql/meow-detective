use std::time::{Duration, Instant};

use transport::dto::{PerformanceMetricDto, PerformanceReportDto, PerformanceReportSummaryDto};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerfSample {
    pub elapsed_ms: u64,
    pub rows: u64,
}

impl PerfSample {
    pub fn rows_per_sec(self) -> Option<f64> {
        rows_per_sec(self.rows, self.elapsed_ms)
    }
}

pub fn measure_rows<T>(rows: u64, operation: impl FnOnce() -> T) -> (T, PerfSample) {
    let started = Instant::now();
    let result = operation();
    let elapsed_ms = elapsed_ms(started.elapsed());
    (result, PerfSample { elapsed_ms, rows })
}

pub fn elapsed_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

pub fn rows_per_sec(rows: u64, elapsed_ms: u64) -> Option<f64> {
    if rows == 0 || elapsed_ms == 0 {
        return None;
    }
    Some(rows as f64 * 1000.0 / elapsed_ms as f64)
}

pub fn metric(key: impl Into<String>, value: f64, unit: impl Into<String>) -> PerformanceMetricDto {
    PerformanceMetricDto {
        key: key.into(),
        value,
        unit: unit.into(),
    }
}

pub fn report(
    report_id: impl Into<String>,
    job_id: Option<String>,
    elapsed_ms: u64,
    summary: impl Into<String>,
    metrics: Vec<PerformanceMetricDto>,
) -> PerformanceReportDto {
    PerformanceReportDto {
        summary: PerformanceReportSummaryDto {
            report_id: report_id.into(),
            job_id,
            generated_at: chrono::Utc::now().to_rfc3339(),
            elapsed_ms,
            peak_memory_bytes: None,
            summary: summary.into(),
        },
        metrics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_per_sec_is_stable_for_fixed_inputs() {
        assert_eq!(rows_per_sec(50, 250), Some(200.0));
        assert_eq!(rows_per_sec(0, 250), None);
        assert_eq!(rows_per_sec(50, 0), None);
    }

    #[test]
    fn elapsed_ms_saturates_to_u64() {
        let duration = Duration::from_millis(125);
        assert_eq!(elapsed_ms(duration), 125);
    }

    #[test]
    fn measure_rows_records_result_and_non_negative_elapsed() {
        let (result, sample) = measure_rows(3, || vec![1, 2, 3]);

        assert_eq!(result, vec![1, 2, 3]);
        assert_eq!(sample.rows, 3);
        assert_eq!(
            sample.rows_per_sec(),
            rows_per_sec(sample.rows, sample.elapsed_ms)
        );
    }

    #[test]
    fn report_uses_stable_metric_shape() {
        let report = report(
            "perf-test",
            None,
            42,
            "Search query returned 2 rows in 42 ms",
            vec![metric("search.query.elapsedMs", 42.0, "ms")],
        );

        assert_eq!(report.summary.report_id, "perf-test");
        assert_eq!(report.summary.elapsed_ms, 42);
        assert_eq!(report.metrics[0].key, "search.query.elapsedMs");
        assert_eq!(report.metrics[0].unit, "ms");
    }
}
