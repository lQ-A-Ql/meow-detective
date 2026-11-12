use std::{io, process::Child};
use transport::dto::bolt_reader::BoltReadLimitsDto;

#[cfg(windows)]
pub(super) struct Job(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Job {
    pub(super) fn completed(&mut self) {
        if !self.0.is_null() {
            // SAFETY: This guard uniquely owns the valid job handle, which is cleared immediately afterward.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
            self.0 = std::ptr::null_mut();
        }
    }

    pub(super) fn attach(child: &Child, limits: &BoltReadLimitsDto) -> io::Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::*;
        // SAFETY: Null security attributes and name create a new unnamed job object.
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Self(handle);
        // SAFETY: The Windows POD information structure permits zero initialization.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | JOB_OBJECT_LIMIT_PROCESS_TIME
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
        info.BasicLimitInformation.ActiveProcessLimit = 1;
        info.BasicLimitInformation.PerProcessUserTimeLimit =
            (limits.max_runtime_millis * 10_000) as i64;
        info.ProcessMemoryLimit = limits.memory_limit_bytes as usize;
        // SAFETY: The job/process handles remain owned and the information buffer has the required initialized layout.
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&info) as u32,
            )
        };
        if configured == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: Child owns this live process handle; assigning the new job does not transfer handle ownership.
        if unsafe { AssignProcessToJobObject(handle, child.as_raw_handle().cast()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }
}

#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        self.completed();
    }
}

#[cfg(unix)]
pub(super) struct Job(Option<i32>);

#[cfg(unix)]
impl Job {
    pub(super) fn attach(child: &Child, _limits: &BoltReadLimitsDto) -> io::Result<Self> {
        Ok(Self(Some(
            i32::try_from(child.id()).map_err(io::Error::other)?,
        )))
    }
    pub(super) fn completed(&mut self) {
        self.0 = None;
    }
}

#[cfg(unix)]
impl Drop for Job {
    fn drop(&mut self) {
        if let Some(group) = self.0 {
            // SAFETY: The unreaped child owns this process group, created explicitly for this request.
            unsafe { libc::kill(-group, libc::SIGKILL) };
        }
    }
}
