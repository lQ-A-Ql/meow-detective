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
