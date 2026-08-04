use std::ptr::{null, null_mut};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_ACCESS_DENIED, ERROR_INSUFFICIENT_BUFFER, ERROR_SERVICE_ALREADY_RUNNING,
};
use windows_sys::Win32::System::Services::{
    ChangeServiceConfigW, CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceConfigW,
    QueryServiceStatusEx, StartServiceW, QUERY_SERVICE_CONFIGW, SC_HANDLE, SC_MANAGER_CONNECT,
    SC_STATUS_PROCESS_INFO, SERVICE_CHANGE_CONFIG, SERVICE_DEMAND_START, SERVICE_DISABLED,
    SERVICE_NO_CHANGE, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_START,
    SERVICE_START_PENDING, SERVICE_STATUS_PROCESS, SERVICE_STOPPED, SERVICE_STOP_PENDING,
};

use crate::PhysicalMountError;

const SERVICE_NAME: &str = "MSiSCSI";
const SERVICE_START_TIMEOUT: Duration = Duration::from_secs(10);

static SERVICE_COORDINATOR: OnceLock<Mutex<ServiceCoordinatorState>> = OnceLock::new();

pub(crate) struct IscsiServiceLease {
    active: bool,
}

impl IscsiServiceLease {
    pub(crate) fn acquire() -> Result<Self, PhysicalMountError> {
        let mut state = service_coordinator()
            .lock()
            .map_err(|_| PhysicalMountError::IscsiServiceCoordinatorPoisoned)?;
        if state.active_leases == 0 {
            let restore_start_type = ensure_iscsi_service_running()?;
            state.register_lease(restore_start_type)?;
        } else {
            state.register_lease(None)?;
        }
        Ok(Self { active: true })
    }

    pub(crate) fn release(&mut self) -> Result<(), PhysicalMountError> {
        if !self.active {
            return Ok(());
        }
        let mut state = service_coordinator()
            .lock()
            .map_err(|_| PhysicalMountError::IscsiServiceCoordinatorPoisoned)?;
        let restore_start_type = state.release_lease()?;
        self.active = false;
        if let Some(start_type) = restore_start_type {
            restore_start_type_if_owned(start_type)?;
            state.restore_start_type = None;
        }
        Ok(())
    }
}

impl Drop for IscsiServiceLease {
    fn drop(&mut self) {
        if let Err(error) = self.release() {
            tracing::error!(error = %error, "Failed to release Microsoft iSCSI service lease");
        }
    }
}

#[derive(Default)]
struct ServiceCoordinatorState {
    active_leases: usize,
    restore_start_type: Option<u32>,
}

impl ServiceCoordinatorState {
    fn register_lease(
        &mut self,
        restore_start_type: Option<u32>,
    ) -> Result<(), PhysicalMountError> {
        self.active_leases = self
            .active_leases
            .checked_add(1)
            .ok_or(PhysicalMountError::IscsiServiceLeaseState)?;
        if self.active_leases == 1 && self.restore_start_type.is_none() {
            self.restore_start_type = restore_start_type;
        }
        Ok(())
    }

    fn release_lease(&mut self) -> Result<Option<u32>, PhysicalMountError> {
        self.active_leases = self
            .active_leases
            .checked_sub(1)
            .ok_or(PhysicalMountError::IscsiServiceLeaseState)?;
        Ok((self.active_leases == 0)
            .then_some(self.restore_start_type)
            .flatten())
    }
}

fn service_coordinator() -> &'static Mutex<ServiceCoordinatorState> {
    SERVICE_COORDINATOR.get_or_init(|| Mutex::new(ServiceCoordinatorState::default()))
}

fn ensure_iscsi_service_running() -> Result<Option<u32>, PhysicalMountError> {
    let manager = ServiceHandle::open_manager()?;
    let service_name = wide_null(SERVICE_NAME);
    let query_service = ServiceHandle::open_service(
        manager.0,
        service_name.as_ptr(),
        SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG,
        "OpenServiceW(query)",
    )?;
    let state = query_state(query_service.0)?;
    if state == SERVICE_RUNNING {
        return Ok(None);
    }
    let original_start_type = query_start_type(query_service.0)?;
    drop(query_service);

    let access = SERVICE_QUERY_STATUS
        | SERVICE_START
        | SERVICE_QUERY_CONFIG
        | if original_start_type == SERVICE_DISABLED {
            SERVICE_CHANGE_CONFIG
        } else {
            0
        };
    let service = ServiceHandle::open_service(
        manager.0,
        service_name.as_ptr(),
        access,
        "OpenServiceW(start)",
    )
    .map_err(map_elevation_error)?;
    let restore_start_type = if original_start_type == SERVICE_DISABLED {
        change_start_type(service.0, SERVICE_DEMAND_START)?;
        Some(SERVICE_DISABLED)
    } else {
        None
    };

    if let Err(error) = start_service_and_wait(service.0, state) {
        if let Some(start_type) = restore_start_type {
            if let Err(rollback_error) = change_start_type(service.0, start_type) {
                tracing::error!(
                    error = %rollback_error,
                    "Failed to roll back Microsoft iSCSI service configuration"
                );
            }
        }
        return Err(error);
    }
    Ok(restore_start_type)
}

fn start_service_and_wait(handle: SC_HANDLE, initial_state: u32) -> Result<(), PhysicalMountError> {
    if initial_state == SERVICE_RUNNING {
        return Ok(());
    }
    if initial_state == SERVICE_STOP_PENDING {
        wait_for_state(handle, SERVICE_STOPPED)?;
    }
    if initial_state != SERVICE_START_PENDING {
        // SAFETY: the service handle has SERVICE_START access and no arguments
        // are passed to the service.
        let started = unsafe { StartServiceW(handle, 0, null()) };
        if started == 0 {
            // SAFETY: GetLastError is read immediately after the failed call.
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
    }
    wait_for_state(handle, SERVICE_RUNNING)
}

fn restore_start_type_if_owned(original_start_type: u32) -> Result<(), PhysicalMountError> {
    let manager = ServiceHandle::open_manager()?;
    let service_name = wide_null(SERVICE_NAME);
    let service = ServiceHandle::open_service(
        manager.0,
        service_name.as_ptr(),
        SERVICE_QUERY_CONFIG | SERVICE_CHANGE_CONFIG,
        "OpenServiceW(restore-config)",
    )
    .map_err(map_elevation_error)?;
    let current_start_type = query_start_type(service.0)?;
    if current_start_type == original_start_type {
        return Ok(());
    }
    if current_start_type != SERVICE_DEMAND_START {
        tracing::warn!(
            current_start_type,
            original_start_type,
            "Microsoft iSCSI service startup type changed externally; preserving external setting"
        );
        return Ok(());
    }
    change_start_type(service.0, original_start_type)
}

fn change_start_type(handle: SC_HANDLE, start_type: u32) -> Result<(), PhysicalMountError> {
    // SAFETY: the service handle has SERVICE_CHANGE_CONFIG access. Null values
    // and SERVICE_NO_CHANGE preserve every field except the startup type.
    let result = unsafe {
        ChangeServiceConfigW(
            handle,
            SERVICE_NO_CHANGE,
            start_type,
            SERVICE_NO_CHANGE,
            null(),
            null(),
            null_mut(),
            null(),
            null(),
            null(),
            null(),
        )
    };
    if result == 0 {
        return Err(map_elevation_error(last_api_error("ChangeServiceConfigW")));
    }
    Ok(())
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

fn query_start_type(handle: SC_HANDLE) -> Result<u32, PhysicalMountError> {
    let mut required = 0u32;
    // SAFETY: a null buffer with size zero is the documented size probe.
    let result = unsafe { QueryServiceConfigW(handle, null_mut(), 0, &mut required) };
    if result == 0 {
        // SAFETY: GetLastError is read immediately after the size probe.
        let code = unsafe { GetLastError() };
        if code != ERROR_INSUFFICIENT_BUFFER {
            return Err(PhysicalMountError::WindowsApi {
                operation: "QueryServiceConfigW(size)",
                code,
            });
        }
    }
    if required < std::mem::size_of::<QUERY_SERVICE_CONFIGW>() as u32 {
        return Err(PhysicalMountError::WindowsApi {
            operation: "QueryServiceConfigW(size)",
            code: ERROR_INSUFFICIENT_BUFFER,
        });
    }
    let word_size = std::mem::size_of::<usize>();
    let words = (required as usize).div_ceil(word_size);
    let mut buffer = vec![0usize; words];
    let config = buffer.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>();
    // SAFETY: the usize buffer is suitably aligned and has at least `required`
    // writable bytes. Pointers in the returned structure remain buffer-local.
    let result = unsafe { QueryServiceConfigW(handle, config, required, &mut required) };
    if result == 0 {
        return Err(last_api_error("QueryServiceConfigW"));
    }
    // SAFETY: QueryServiceConfigW initialized the fixed structure prefix.
    Ok(unsafe { (*config).dwStartType })
}

fn wait_for_state(handle: SC_HANDLE, expected: u32) -> Result<(), PhysicalMountError> {
    let deadline = Instant::now() + SERVICE_START_TIMEOUT;
    while Instant::now() < deadline {
        if query_state(handle)? == expected {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(PhysicalMountError::IscsiServiceStartupTimeout)
}

fn map_elevation_error(error: PhysicalMountError) -> PhysicalMountError {
    match error {
        PhysicalMountError::WindowsApi {
            code: ERROR_ACCESS_DENIED,
            ..
        } => PhysicalMountError::IscsiServiceRequiresElevation,
        other => other,
    }
}

fn last_api_error(operation: &'static str) -> PhysicalMountError {
    // SAFETY: called immediately after the failing Windows API operation.
    let code = unsafe { GetLastError() };
    PhysicalMountError::WindowsApi { operation, code }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
#[path = "../tests/unit/windows_service.rs"]
mod tests;
