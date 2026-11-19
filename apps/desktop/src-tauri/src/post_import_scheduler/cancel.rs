//! Cancel APIs: single capability task and whole-case teardown.

use super::state::{condvar, lock};
use super::QueuedCapability;

/// Cancel one queued or running capability task. Returns `true` when a live
/// running token was signalled; queued entries are removed either way.
pub(crate) fn cancel_source(case_id: &str, source_id: &str, capability: QueuedCapability) -> bool {
    let family = format!("{case_id}|{source_id}|{}", capability.wire());
    let prefix = format!("{family}|");
    let mut guard = lock();
    guard.queue.retain(|task| task.family_key() != family);
    let mut signalled = false;
    for (key, token) in guard.active_tokens.iter() {
        if key.starts_with(&prefix) {
            token.store(true, std::sync::atomic::Ordering::SeqCst);
            signalled = true;
        }
    }
    signalled
}

/// Cancel every queued/running post-processing task belonging to a closing
/// case. Running tasks observe the token at the next chunk boundary; queued
/// entries are dropped from memory.
pub(crate) fn cancel_case(case_id: &str) {
    let prefix = format!("{case_id}|");
    let mut guard = lock();
    guard.queue.retain(|task| task.case_id != case_id);
    for (key, token) in guard.active_tokens.iter() {
        if key.starts_with(&prefix) {
            token.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
    drop(guard);
    condvar().notify_all();
}
