use super::*;

#[test]
fn positive_retryable_failure_count_maps_to_partial() {
    let outcome = artifact_capability_state(3, 2);
    assert_eq!(outcome.state, "partial");
    assert_eq!(outcome.error_code, Some("EXTRACTION_PARTIAL"));
    let detail = outcome
        .detail
        .expect("partial row carries a readable reason");
    assert!(detail.contains("retryableFailures=3"), "detail: {detail}");
    assert!(detail.contains("warnings=2"), "detail: {detail}");
}

#[test]
fn zero_retryable_failure_count_maps_to_ready() {
    let outcome = artifact_capability_state(0, 5);
    assert_eq!(outcome.state, "ready");
    assert!(outcome.detail.is_none());
    assert!(outcome.error_code.is_none());
}

#[test]
fn zero_retryable_and_zero_warnings_maps_to_ready() {
    let outcome = artifact_capability_state(0, 0);
    assert_eq!(outcome.state, "ready");
    assert!(outcome.detail.is_none());
    assert!(outcome.error_code.is_none());
}

#[test]
fn partial_never_reports_ready() {
    for failures in 1..=4u64 {
        let outcome = artifact_capability_state(failures, 0);
        assert_eq!(
            outcome.state, "partial",
            "failures={failures} must not be ready"
        );
    }
}
