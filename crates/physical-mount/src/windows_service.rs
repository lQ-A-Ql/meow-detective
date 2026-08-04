use std::ptr::null;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_ACCESS_DENIED, ERROR_SERVICE_ALREADY_RUNNING,
};
use windows_sys::Win32::System::Services::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatusEx, StartServiceW,
    SC_HANDLE, SC_MANAGER_CONNECT, SC_STATUS_PROCESS_INFO, SERVICE_QUERY_STATUS, SERVICE_RUNNING,
    SERVICE_START, SERVICE_STATUS_PROCESS,
};

use crate::PhysicalMountError;

const SERVICE_START_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn ensure_iscsi_service_running() -> Result<(), PhysicalMountError> {
    let manager = ServiceHandle::open_manager()?;
    let service_name = wide_null("MSiSCSI");
    let service = ServiceHandle::open_service(
        manager.0,
        service_name.as_ptr(),
        SERVICE_QUERY_STATUS,
        "OpenServiceW(query)",
    )?;
    if query_state(service.0)? == SERVICE_RUNNING {
        return Ok(());
    }
    drop(service);

    let service = match ServiceHandle::open_service(
        manager.0,
        service_name.as_ptr(),
        SERVICE_QUERY_STATUS | SERVICE_START,
        "OpenServiceW(start)",
    ) {
        Err(PhysicalMountError::WindowsApi {
            code: ERROR_ACCESS_DENIED,
            ..
        }) => return Err(PhysicalMountError::IscsiServiceRequiresElevation),
        result => result?,
    };
    // SAFETY: the service handle has SERVICE_START access and no arguments are
    // passed to the service.
    let started = unsafe { StartServiceW(service.0, 0, null()) };
    if started == 0 {
        // SAFETY: GetLastError is read immediately after the failing API call.
        let code = unsafe { GetLastError() };
        if code == ERROR_ACCESS_DENIED {
            return Err(PhysicalMountError::IscsiServiceRequiresElevation);
        }
        if code != ERROR_SERVICE_ALREADY_RUNNING {
            return Err(PhysicalMountError::WindowsApi {
                operation: "StartServiceW",
                code,
            });
        }
    }
    let deadline = Instant::now() + SERVICE_START_TIMEOUT;
    while Instant::now() < deadline {
        if query_state(service.0)? == SERVICE_RUNNING {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(PhysicalMountError::IscsiServiceStartupTimeout)
}

struct ServiceHandle(SC_HANDLE);

impl ServiceHandle {
    fn open_manager() -> Result<Self, PhysicalMountError> {
        // SAFETY: null machine/database pointers select the local default SCM.
        let handle = unsafe { OpenSCManagerW(null(), null(), SC_MANAGER_CONNECT) };
        if handle.is_null() {
            return Err(last_api_error("OpenSCManagerW"));
        }
        Ok(Self(handle))
    }

    fn open_service(
        manager: SC_HANDLE,
        service_name: *const u16,
        access: u32,
        operation: &'static str,
    ) -> Result<Self, PhysicalMountError> {
        // SAFETY: manager is a valid SCM handle and service_name points to a
        // null-terminated string that remains alive for the synchronous call.
        let handle = unsafe { OpenServiceW(manager, service_name, access) };
        if handle.is_null() {
            return Err(last_api_error(operation));
        }
        Ok(Self(handle))
    }
}

impl Drop for ServiceHandle {
    fn drop(&mut self) {
        // SAFETY: the handle is owned by this guard and closed exactly once.
        let _ = unsafe { CloseServiceHandle(self.0) };
    }
}

fn query_state(handle: SC_HANDLE) -> Result<u32, PhysicalMountError> {
    let mut status = SERVICE_STATUS_PROCESS::default();
    let mut required = 0u32;
    // SAFETY: the output buffer is correctly sized and writable.
    let result = unsafe {
        QueryServiceStatusEx(
            handle,
            SC_STATUS_PROCESS_INFO,
            (&mut status as *mut SERVICE_STATUS_PROCESS).cast::<u8>(),
            std::mem::size_of::<SERVICE_STATUS_PROCESS>() as u32,
            &mut required,
        )
    };
    if result == 0 {
        return Err(last_api_error("QueryServiceStatusEx"));
    }
    Ok(status.dwCurrentState)
}

fn last_api_error(operation: &'static str) -> PhysicalMountError {
    // SAFETY: called immediately after the failing Windows API operation.
    let code = unsafe { GetLastError() };
    PhysicalMountError::WindowsApi { operation, code }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
