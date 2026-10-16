use std::time::Duration;

use transport::dto::import_benchmark::{
    ImportBenchmarkProcessScopeDto, ImportBenchmarkProcessUsageDto,
};

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ProcessIoCounters {
    pub read_operations: u64,
    pub write_operations: u64,
    pub other_operations: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub other_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ProcessCounterSnapshot {
    pub user_cpu_ns: Option<u64>,
    pub kernel_cpu_ns: Option<u64>,
    pub io: Option<ProcessIoCounters>,
    pub handles: Option<u32>,
}

impl ProcessCounterSnapshot {
    pub(crate) fn capture() -> Self {
        #[cfg(target_os = "windows")]
        {
            capture_windows()
        }
        #[cfg(not(target_os = "windows"))]
        {
            Self::default()
        }
    }

    pub(crate) fn usage_until(self, end: Self, wall: Duration) -> ImportBenchmarkProcessUsageDto {
        let user_cpu_ns = delta(self.user_cpu_ns, end.user_cpu_ns);
        let kernel_cpu_ns = delta(self.kernel_cpu_ns, end.kernel_cpu_ns);
        let cpu_core_equivalents = user_cpu_ns
            .zip(kernel_cpu_ns)
            .and_then(|(user, kernel)| user.checked_add(kernel))
            .filter(|_| !wall.is_zero())
            .map(|cpu| cpu as f64 / wall.as_nanos() as f64);
        ImportBenchmarkProcessUsageDto {
            scope: ImportBenchmarkProcessScopeDto::WorkflowProcessWindow,
            wall_time_ns: wall.as_nanos().min(u64::MAX as u128) as u64,
            user_cpu_ns,
            kernel_cpu_ns,
            cpu_core_equivalents,
            process_read_transfer_bytes: delta(
                self.io.map(|io| io.read_bytes),
                end.io.map(|io| io.read_bytes),
            ),
            process_write_transfer_bytes: delta(
                self.io.map(|io| io.write_bytes),
                end.io.map(|io| io.write_bytes),
            ),
            process_read_operations: delta(
                self.io.map(|io| io.read_operations),
                end.io.map(|io| io.read_operations),
            ),
            process_write_operations: delta(
                self.io.map(|io| io.write_operations),
                end.io.map(|io| io.write_operations),
            ),
            handles_at_start: self.handles,
            handles_at_end: end.handles,
        }
    }
}

fn delta(start: Option<u64>, end: Option<u64>) -> Option<u64> {
    start
        .zip(end)
        .and_then(|(start, end)| end.checked_sub(start))
}

#[cfg(target_os = "windows")]
fn capture_windows() -> ProcessCounterSnapshot {
    use windows_sys::Win32::Foundation::FILETIME;
    #[link(name = "Kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn GetProcessTimes(
            process: *mut std::ffi::c_void,
            creation: *mut FILETIME,
            exit: *mut FILETIME,
            kernel: *mut FILETIME,
            user: *mut FILETIME,
        ) -> i32;
        fn GetProcessIoCounters(process: *mut std::ffi::c_void, io: *mut ProcessIoCounters) -> i32;
        fn GetProcessHandleCount(process: *mut std::ffi::c_void, handles: *mut u32) -> i32;
    }
    let zero_time = || FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut creation, mut exit, mut kernel, mut user) =
        (zero_time(), zero_time(), zero_time(), zero_time());
    let mut io = ProcessIoCounters::default();
    let mut handles = 0;
    // SAFETY: the current-process pseudo handle is valid and is not closed.
    // Output pointers reference initialized, correctly aligned Windows ABI structures.
    let (times_ok, io_ok, handles_ok) = unsafe {
        let process = GetCurrentProcess();
        (
            GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user),
            GetProcessIoCounters(process, &mut io),
            GetProcessHandleCount(process, &mut handles),
        )
    };
    let nanos = |time: FILETIME| {
        ((u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)).checked_mul(100)
    };
    ProcessCounterSnapshot {
        user_cpu_ns: (times_ok != 0).then(|| nanos(user)).flatten(),
        kernel_cpu_ns: (times_ok != 0).then(|| nanos(kernel)).flatten(),
        io: (io_ok != 0).then_some(io),
        handles: (handles_ok != 0).then_some(handles),
    }
}
