use crate::timeline_service::fjall::restored_skip_remaining;

#[test]
fn saved_after_key_skips_the_consumed_prefix_only_when_present() {
    assert_eq!(restored_skip_remaining(5, true), 0);
    assert_eq!(restored_skip_remaining(5, false), 5);
    assert_eq!(restored_skip_remaining(0, false), 0);
}
