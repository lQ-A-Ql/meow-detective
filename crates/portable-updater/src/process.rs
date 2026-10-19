use crate::UpdateError;

#[cfg(windows)]
pub struct Parent(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Parent {
    pub fn open(pid: u32) -> Result<Self, UpdateError> {
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE};
        // SAFETY: Requests only SYNCHRONIZE for the parent PID; handle is owned by RAII.
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self(handle))
    }
    pub fn wait(&self) -> Result<(), UpdateError> {
        // SAFETY: The owned process handle remains live for the bounded wait.
        let result =
            unsafe { windows_sys::Win32::System::Threading::WaitForSingleObject(self.0, 60_000) };
        if result != 0 {
            return Err(UpdateError::Invalid(
                "parent did not exit within 60 seconds",
            ));
        }
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for Parent {
    fn drop(&mut self) {
        // SAFETY: Sole owner of the non-null handle returned by OpenProcess.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(not(windows))]
pub struct Parent;
#[cfg(not(windows))]
impl Parent {
    pub fn open(_: u32) -> Result<Self, UpdateError> {
        Err(UpdateError::Invalid(
            "portable updates support Windows x64 only",
        ))
    }
    pub fn wait(&self) -> Result<(), UpdateError> {
        Err(UpdateError::Invalid("unsupported platform"))
    }
}
