#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

pub fn current_rss_mb() -> u64 {
    #[cfg(test)]
    if let Some(value) = test_rss_override_mb() {
        return value;
    }
    current_rss_bytes() / (1024 * 1024)
}

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

pub(super) fn memory_hard_limit_exceeded(limit_mb: u64) -> bool {
    let rss_mb = current_rss_mb();
    rss_mb > 0 && rss_mb >= limit_mb
}

#[cfg(test)]
static TEST_RSS_OVERRIDE_MB: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
fn test_rss_override_mb() -> Option<u64> {
    match TEST_RSS_OVERRIDE_MB.load(Ordering::Relaxed) {
        0 => None,
        value => Some(value),
    }
}

#[cfg(test)]
pub(super) fn set_test_rss_override_mb(value: Option<u64>) {
    TEST_RSS_OVERRIDE_MB.store(value.unwrap_or(0), Ordering::Relaxed);
}

#[cfg(target_os = "windows")]
fn current_rss_bytes() -> u64 {
    #[repr(C)]
    #[allow(non_snake_case)]
    struct ProcessMemoryCounters {
        cb: u32,
        PageFaultCount: u32,
        PeakWorkingSetSize: usize,
        WorkingSetSize: usize,
        QuotaPeakPagedPoolUsage: usize,
        QuotaPagedPoolUsage: usize,
        QuotaPeakNonPagedPoolUsage: usize,
        QuotaNonPagedPoolUsage: usize,
        PagefileUsage: usize,
        PeakPagefileUsage: usize,
    }

    extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            counters: *mut ProcessMemoryCounters,
            size: u32,
        ) -> i32;
    }

    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        PageFaultCount: 0,
        PeakWorkingSetSize: 0,
        WorkingSetSize: 0,
        QuotaPeakPagedPoolUsage: 0,
        QuotaPagedPoolUsage: 0,
        QuotaPeakNonPagedPoolUsage: 0,
        QuotaNonPagedPoolUsage: 0,
        PagefileUsage: 0,
        PeakPagefileUsage: 0,
    };
    // SAFETY: GetProcessMemoryInfo is called with a valid handle (GetCurrentProcess)
    // and a properly initialized PROCESS_MEMORY_COUNTERS struct. The Windows API
    // guarantees these are safe to call from any thread.
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<ProcessMemoryCounters>() as u32,
        )
    };
    if ok == 0 {
        0
    } else {
        counters.WorkingSetSize as u64
    }
}

#[cfg(not(target_os = "windows"))]
fn current_rss_bytes() -> u64 {
    0
}
