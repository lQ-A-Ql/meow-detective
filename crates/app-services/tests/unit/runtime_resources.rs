use super::memory_hard_limit_exceeded_for_rss;

#[test]
fn hard_limit_requires_a_nonzero_rss_at_or_above_the_limit() {
    assert!(memory_hard_limit_exceeded_for_rss(3, 2));
    assert!(memory_hard_limit_exceeded_for_rss(2, 2));
    assert!(!memory_hard_limit_exceeded_for_rss(1, 2));
    assert!(!memory_hard_limit_exceeded_for_rss(0, 0));
}
