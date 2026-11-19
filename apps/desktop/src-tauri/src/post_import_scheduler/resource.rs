//! L4 resource coordination helpers for the post-import scheduler.
//!
//! * `PostMemoryGuard`: a counting semaphore used only as a per-task
//!   concurrency counter. It is deliberately NOT wired into the 1536 MiB
//!   import capacity of `import_core::import_scheduler`; the constant below is
//!   a placeholder pending calibration.
//! * `DEGRADED_SLOT`: after the import-priority grace expires, at most one
//!   post-import worker may run while imports stay active, so queued work
//!   makes progress without starving forever.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

/// Post-import per-task memory reservation in MiB. L4 uses this only as a
/// single-task concurrency counter; it does NOT feed the 1536 MiB import
/// capacity. TODO(calibrate): 256 MiB placeholder pending benchmark measurement.
pub(super) const POST_IMPORT_MEMORY_RESERVATION_MB: u64 = 256;

/// Import-priority grace: post-import work yields to an active import for at
/// most this long before degrading to a single worker. TODO(calibrate): 300s
/// placeholder pending benchmark measurement.
pub(super) const POST_IMPORT_IMPORT_PRIORITY_WAIT: Duration = Duration::from_secs(300);

/// Natural per-case auto-task upper bound: each source exposes at most two
/// capabilities (`timeline` + `artifacts`), so the per-case auto queue length
/// is bounded by 2 × sources. Enforced as family-key dedup in `state::admit`;
/// duplicates are dropped silently instead of raising an error.
pub(super) const MAX_AUTO_TASKS_PER_CASE: usize = 2;

struct MemorySemaphore {
    available: AtomicUsize,
    gate: Mutex<()>,
    changed: Condvar,
}

fn memory_semaphore() -> &'static MemorySemaphore {
    static SEMAPHORE: OnceLock<MemorySemaphore> = OnceLock::new();
    SEMAPHORE.get_or_init(|| MemorySemaphore {
        available: AtomicUsize::new(POST_IMPORT_MEMORY_RESERVATION_MB as usize),
        gate: Mutex::new(()),
        changed: Condvar::new(),
    })
}

/// RAII unit of the post-import memory semaphore. Released on Drop.
pub(super) struct PostMemoryGuard {
    _private: (),
}

impl PostMemoryGuard {
    /// Acquire one semaphore unit, polling every 500ms while exhausted. The
    /// pool only ever holds the bounded worker pool, so this wait is a
    /// safety hook rather than a hot path.
    pub(super) fn acquire() -> Self {
        let semaphore = memory_semaphore();
        loop {
            let acquired =
                semaphore
                    .available
                    .try_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                        value.checked_sub(1)
                    });
            match acquired {
                Ok(_) => return Self { _private: () },
                Err(_) => {
                    let lock = semaphore
                        .gate
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    let result = semaphore
                        .changed
                        .wait_timeout(lock, Duration::from_millis(500));
                    if let Err(poisoned) = result {
                        let _ = poisoned.into_inner();
                    }
                }
            }
        }
    }
}

impl Drop for PostMemoryGuard {
    fn drop(&mut self) {
        let semaphore = memory_semaphore();
        semaphore.available.fetch_add(1, Ordering::AcqRel);
        semaphore.changed.notify_all();
    }
}

/// Single degraded slot: 0 = free, 1 = held by the one worker allowed past
/// the import-priority grace while imports remain active.
static DEGRADED_SLOT: AtomicUsize = AtomicUsize::new(0);

/// CAS-acquire the degraded single-worker slot. Returns `false` when another
/// worker already holds it.
pub(super) fn try_take_degraded_slot() -> bool {
    DEGRADED_SLOT
        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

/// Release the degraded slot after the task finishes.
pub(super) fn release_degraded_slot() {
    DEGRADED_SLOT.store(0, Ordering::Release);
}
