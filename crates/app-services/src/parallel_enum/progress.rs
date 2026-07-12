pub(super) const PROGRESS_CHANNEL_CAPACITY: usize = 128;

pub(super) const ENUM_PROGRESS_INTERVAL: u64 = 5_000;

pub(super) fn heartbeat_percent(done_count: usize, submitted_count: usize, entries: u64) -> u32 {
    if submitted_count == 0 {
        return 0;
    }

    let base = ((done_count as u32 * 100) / submitted_count as u32).min(99);
    if entries > 0 {
        base.clamp(3, 99)
    } else {
        base
    }
}
