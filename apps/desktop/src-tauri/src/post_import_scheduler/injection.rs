//! Test-only post-import execution delay (`MEOW_POST_IMPORT_DELAY_MS`).
//!
//! The hook exists for manual acceptance of a slow timeline: set the env var
//! to a positive millisecond count before launching; the worker sleeps that
//! long before executing a claimed task. Default (unset / 0 / invalid) is 0,
//! so production behavior never changes.

use std::time::Duration;

pub const POST_IMPORT_DELAY_ENV: &str = "MEOW_POST_IMPORT_DELAY_MS";

/// Parse the optional injection delay in milliseconds. Unset, empty, zero, or
/// non-numeric input resolves to 0 (no delay); negative values also fail u64
/// parsing and resolve to 0.
pub fn parse_post_import_delay_ms(raw: Option<&str>) -> u64 {
    raw.and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0)
}

/// Read the delay from the process environment.
pub fn post_import_delay_ms() -> u64 {
    parse_post_import_delay_ms(std::env::var(POST_IMPORT_DELAY_ENV).ok().as_deref())
}

/// Sleep for the configured injection delay before task execution. No-op at 0,
/// so the unset production default adds no latency.
pub(crate) fn apply_injection_delay() {
    let delay = post_import_delay_ms();
    if delay > 0 {
        std::thread::sleep(Duration::from_millis(delay));
    }
}

#[cfg(test)]
#[path = "../../tests/unit/post_import_scheduler/injection.rs"]
mod tests;
