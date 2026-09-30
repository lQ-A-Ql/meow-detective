use super::*;

#[test]
fn worker_count_is_bounded_and_never_zero() {
    assert_eq!(worker_count(0), 1);
    assert!(worker_count(10_000) <= 64);
    assert!(worker_count(3) <= 3);
}
