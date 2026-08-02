use std::time::Duration;

pub(super) use crate::runtime_resources::{current_rss_mb, memory_hard_limit_exceeded};

pub(super) fn rows_per_sec(rows: u64, duration: Duration) -> u64 {
    let secs = duration.as_secs_f64();
    if secs <= 0.0 {
        rows
    } else {
        (rows as f64 / secs).round() as u64
    }
}

pub(super) fn bool_word(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

pub(super) fn scheduling_state(
    cancel_requested: bool,
    rss_mb: u64,
    memory_soft_limit_mb: u64,
) -> &'static str {
    if cancel_requested {
        "draining"
    } else if rss_mb >= memory_soft_limit_mb {
        "throttled"
    } else {
        "running"
    }
}
