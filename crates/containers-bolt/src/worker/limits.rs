use std::io;
use transport::dto::bolt_reader::BoltReadLimitsDto;

#[cfg(unix)]
pub(super) fn apply(limits: &BoltReadLimitsDto) -> io::Result<()> {
    let memory = libc::rlimit {
        rlim_cur: limits.memory_limit_bytes,
        rlim_max: limits.memory_limit_bytes,
    };
    let seconds = limits.max_runtime_millis.div_ceil(1000) + 1;
    let cpu = libc::rlimit {
        rlim_cur: seconds,
        rlim_max: seconds,
    };
    // SAFETY: Both pointers reference initialized rlimit structures for the duration of each syscall.
    let result = unsafe {
        (
            libc::setrlimit(libc::RLIMIT_AS, &memory),
            libc::setrlimit(libc::RLIMIT_CPU, &cpu),
        )
    };
    if result.0 != 0 || result.1 != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn apply(limits: &BoltReadLimitsDto) -> io::Result<()> {
    use windows_sys::Win32::System::JobObjects::*;
    // SAFETY: The Windows POD information structure permits zero initialization.
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: A null job handle queries this process's enclosing job; the writable buffer has the required size.
    let success = unsafe {
        QueryInformationJobObject(
            std::ptr::null_mut(),
            JobObjectExtendedLimitInformation,
            (&mut info as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&info) as u32,
            std::ptr::null_mut(),
        )
    };
    if success == 0 {
        return Err(io::Error::last_os_error());
    }
    let required = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | JOB_OBJECT_LIMIT_PROCESS_MEMORY
        | JOB_OBJECT_LIMIT_PROCESS_TIME;
    if info.BasicLimitInformation.LimitFlags & required != required
        || info.ProcessMemoryLimit as u64 > limits.memory_limit_bytes
        || info.BasicLimitInformation.PerProcessUserTimeLimit
            > (limits.max_runtime_millis * 10_000) as i64
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "worker resource controls are unavailable",
        ));
    }
    Ok(())
}
