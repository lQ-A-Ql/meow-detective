use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_INSUFFICIENT_BUFFER, ERROR_MORE_DATA,
};
use windows_sys::Win32::Storage::IscsiDisc::{
    AddIScsiStaticTargetW, GetDevicesForIScsiSessionW, LoginIScsiTargetW, LogoutIScsiTarget,
    RemoveIScsiStaticTargetW, ISCSI_CHAP_AUTH_TYPE, ISCSI_DEVICE_ON_SESSIONW,
    ISCSI_DIGEST_TYPE_NONE, ISCSI_LOGIN_OPTIONS, ISCSI_LOGIN_OPTIONS_VERSION, ISCSI_TARGET_PORTALW,
    ISCSI_TARGET_PORTAL_GROUPW, ISCSI_UNIQUE_SESSION_ID,
};

use crate::target::TargetConnection;
use crate::PhysicalMountError;

const ISCSI_ANY_INITIATOR_PORT: u32 = u32::MAX;
const LOGIN_INFO_USERNAME: u32 = 0x20;
const LOGIN_INFO_PASSWORD: u32 = 0x40;
const LOGIN_INFO_AUTH_TYPE: u32 = 0x80;
const DEVICE_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) struct WindowsIscsiSession {
    session_id: ISCSI_UNIQUE_SESSION_ID,
    device_paths: Vec<String>,
    connected: bool,
    static_target: Option<StaticTargetGuard>,
}

impl WindowsIscsiSession {
    pub(crate) fn connect(connection: &TargetConnection) -> Result<Self, PhysicalMountError> {
        let target_name = wide_null(&connection.iqn);
        let mut portal = portal(connection)?;
        let mut username = connection.chap_username.as_bytes().to_vec();
        let mut password = connection.chap_secret.as_bytes().to_vec();
        let mut login_options = login_options(&mut username, &mut password);
        let mut portal_group = ISCSI_TARGET_PORTAL_GROUPW {
            Count: 1,
            Portals: [portal],
        };
        let static_target =
            StaticTargetGuard::register(&target_name, &mut login_options, &mut portal_group)?;
        let mut session_id = ISCSI_UNIQUE_SESSION_ID::default();
        let mut connection_id = ISCSI_UNIQUE_SESSION_ID::default();
        // SAFETY: input buffers and output IDs are valid for the duration of
        // the call. No mappings or preshared IPsec key are requested.
        let login_result = unsafe {
            LoginIScsiTargetW(
                target_name.as_ptr(),
                false,
                null(),
                ISCSI_ANY_INITIATOR_PORT,
                &mut portal,
                0,
                null_mut(),
                &mut login_options,
                0,
                null(),
                false,
                &mut session_id,
                &mut connection_id,
            )
        };
        if login_result != 0 {
            if login_result == ERROR_ACCESS_DENIED {
                return Err(PhysicalMountError::IscsiLoginRequiresElevation);
            }
            return Err(PhysicalMountError::WindowsApi {
                operation: "LoginIScsiTargetW",
                code: login_result,
            });
        }
        let device_paths = match wait_for_device_paths(&mut session_id) {
            Ok(paths) => paths,
            Err(error) => {
                // SAFETY: `session_id` was returned by a successful login.
                let _ = unsafe { LogoutIScsiTarget(&mut session_id) };
                return Err(error);
            }
        };
        Ok(Self {
            session_id,
            device_paths,
            connected: true,
            static_target: Some(static_target),
        })
    }

    pub(crate) fn primary_device_path(&self) -> Option<&str> {
        self.device_paths.first().map(String::as_str)
    }

    pub(crate) fn disconnect(&mut self) -> Result<(), PhysicalMountError> {
        if !self.connected {
            return Ok(());
        }
        let logout_result = logout(&mut self.session_id);
        self.connected = false;
        let remove_result = self
            .static_target
            .take()
            .map(|mut target| target.remove())
            .unwrap_or(Ok(()));
        logout_result.and(remove_result)
    }
}

impl Drop for WindowsIscsiSession {
    fn drop(&mut self) {
        let _ = self.disconnect();
    }
}

struct StaticTargetGuard {
    target_name: Vec<u16>,
    registered: bool,
}

impl StaticTargetGuard {
    fn register(
        target_name: &[u16],
        login_options: &mut ISCSI_LOGIN_OPTIONS,
        portal_group: &mut ISCSI_TARGET_PORTAL_GROUPW,
    ) -> Result<Self, PhysicalMountError> {
        // SAFETY: all pointers reference initialized buffers that remain alive
        // for the synchronous call. The target is explicitly removed on stop.
        let result = unsafe {
            AddIScsiStaticTargetW(
                target_name.as_ptr(),
                null(),
                0,
                false,
                null_mut(),
                login_options,
                portal_group,
            )
        };
        if result == ERROR_ACCESS_DENIED {
            return Err(PhysicalMountError::IscsiLoginRequiresElevation);
        }
        if result != 0 {
            return Err(PhysicalMountError::WindowsApi {
                operation: "AddIScsiStaticTargetW",
                code: result,
            });
        }
        Ok(Self {
            target_name: target_name.to_vec(),
            registered: true,
        })
    }

    fn remove(&mut self) -> Result<(), PhysicalMountError> {
        if !self.registered {
            return Ok(());
        }
        // SAFETY: target_name remains a valid null-terminated buffer until the
        // synchronous API call returns.
        let result = unsafe { RemoveIScsiStaticTargetW(self.target_name.as_ptr()) };
        if result != 0 {
            return Err(PhysicalMountError::WindowsApi {
                operation: "RemoveIScsiStaticTargetW",
                code: result,
            });
        }
        self.registered = false;
        Ok(())
    }
}

impl Drop for StaticTargetGuard {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}

fn logout(session_id: &mut ISCSI_UNIQUE_SESSION_ID) -> Result<(), PhysicalMountError> {
    // SAFETY: the ID belongs to this live session and is only logged out once.
    let result = unsafe { LogoutIScsiTarget(session_id) };
    if result != 0 {
        return Err(PhysicalMountError::WindowsApi {
            operation: "LogoutIScsiTarget",
            code: result,
        });
    }
    Ok(())
}

fn login_options(username: &mut [u8], password: &mut [u8]) -> ISCSI_LOGIN_OPTIONS {
    ISCSI_LOGIN_OPTIONS {
        Version: ISCSI_LOGIN_OPTIONS_VERSION,
        InformationSpecified: LOGIN_INFO_USERNAME | LOGIN_INFO_PASSWORD | LOGIN_INFO_AUTH_TYPE,
        LoginFlags: 0,
        AuthType: ISCSI_CHAP_AUTH_TYPE,
        HeaderDigest: ISCSI_DIGEST_TYPE_NONE,
        DataDigest: ISCSI_DIGEST_TYPE_NONE,
        MaximumConnections: 1,
        DefaultTime2Wait: 0,
        DefaultTime2Retain: 0,
        UsernameLength: username.len() as u32,
        PasswordLength: password.len() as u32,
        Username: username.as_mut_ptr(),
        Password: password.as_mut_ptr(),
    }
}

fn portal(connection: &TargetConnection) -> Result<ISCSI_TARGET_PORTALW, PhysicalMountError> {
    let mut portal = ISCSI_TARGET_PORTALW::default();
    copy_wide_array(&mut portal.SymbolicName, "Meow~Detective loopback")?;
    copy_wide_array(&mut portal.Address, &connection.address)?;
    portal.Socket = connection.port;
    Ok(portal)
}

fn wait_for_device_paths(
    session_id: &mut ISCSI_UNIQUE_SESSION_ID,
) -> Result<Vec<String>, PhysicalMountError> {
    let deadline = Instant::now() + DEVICE_DISCOVERY_TIMEOUT;
    while Instant::now() < deadline {
        let paths = device_paths(session_id)?;
        if !paths.is_empty() {
            return Ok(paths);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(PhysicalMountError::PhysicalDiskNotFound)
}

fn device_paths(
    session_id: &mut ISCSI_UNIQUE_SESSION_ID,
) -> Result<Vec<String>, PhysicalMountError> {
    let mut count = 0u32;
    // SAFETY: the null output buffer is used only to query the required count.
    let probe = unsafe { GetDevicesForIScsiSessionW(session_id, &mut count, null_mut()) };
    if probe == 0 && count == 0 {
        return Ok(Vec::new());
    }
    if probe != ERROR_INSUFFICIENT_BUFFER && probe != ERROR_MORE_DATA {
        return Err(PhysicalMountError::WindowsApi {
            operation: "GetDevicesForIScsiSessionW",
            code: probe,
        });
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    let mut devices = vec![ISCSI_DEVICE_ON_SESSIONW::default(); count as usize];
    // SAFETY: `devices` has room for `count` initialized structures.
    let result =
        unsafe { GetDevicesForIScsiSessionW(session_id, &mut count, devices.as_mut_ptr()) };
    if result != 0 {
        return Err(PhysicalMountError::WindowsApi {
            operation: "GetDevicesForIScsiSessionW",
            code: result,
        });
    }
    devices.truncate(count as usize);
    Ok(devices
        .iter()
        .filter_map(|device| {
            let legacy = wide_array_to_string(&device.LegacyName);
            if !legacy.is_empty() {
                return Some(legacy);
            }
            let interface = wide_array_to_string(&device.DeviceInterfaceName);
            (!interface.is_empty()).then_some(interface)
        })
        .collect())
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn copy_wide_array<const N: usize>(
    target: &mut [u16; N],
    value: &str,
) -> Result<(), PhysicalMountError> {
    let encoded = value.encode_utf16().collect::<Vec<_>>();
    if encoded.len() >= N {
        return Err(PhysicalMountError::PortalValueTooLong("portal string"));
    }
    target[..encoded.len()].copy_from_slice(&encoded);
    target[encoded.len()] = 0;
    Ok(())
}

fn wide_array_to_string<const N: usize>(value: &[u16; N]) -> String {
    let length = value.iter().position(|unit| *unit == 0).unwrap_or(N);
    String::from_utf16_lossy(&value[..length])
}
