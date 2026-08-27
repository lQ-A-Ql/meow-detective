use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use std::{fs, io};

use thiserror::Error;

// VMware may spend several seconds opening the sparse COW-backed disk before
// `vmrun start` reports success. Startup gets its own window so a slow source
// does not look like a failed launch.
const START_TIMEOUT: Duration = Duration::from_secs(120);
const CONTROL_TIMEOUT: Duration = Duration::from_secs(60);
const QUERY_TIMEOUT: Duration = Duration::from_secs(10);
const START_CONFIRM_TIMEOUT: Duration = Duration::from_secs(15);
const START_CONFIRM_INTERVAL: Duration = Duration::from_millis(250);
const SOFT_STOP_POLL_INTERVAL: Duration = Duration::from_secs(1);
const SOFT_STOP_CONFIRM_TIMEOUT: Duration = Duration::from_secs(20);
const VMX_EXIT_CONFIRM_TIMEOUT: Duration = Duration::from_secs(30);
const VMX_EXIT_CONFIRM_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Error)]
pub(super) enum VmwareError {
    #[error("VMware Workstation and vmrun were not found in a supported installation directory")]
    NotInstalled,
    #[error("VMware process could not be started: {0}")]
    Start(std::io::Error),
    #[error("VMware {operation} command timed out after {timeout_secs}s")]
    ControlTimeout {
        operation: String,
        timeout_secs: u64,
    },
    #[error("VMware control command failed with status {0}")]
    ControlFailed(ExitStatus),
    #[error("the VMX path is not valid Unicode and cannot be matched against vmrun output")]
    NonUnicodePath,
    #[error("VMware VMX exit could not be confirmed within {timeout_secs}s")]
    VmxExitTimeout { timeout_secs: u64 },
    #[error("VMware VMX log could not be read while confirming shutdown")]
    VmxLogRead(#[source] io::Error),
}

#[derive(Clone)]
pub(super) struct VmwareControl {
    vmrun: PathBuf,
    vmx: PathBuf,
    vmx_log_baseline: Option<u64>,
}

impl VmwareControl {
    pub(super) fn stop_soft(&self) -> Result<(), VmwareError> {
        self.run_control("stop", Some("soft"))
    }

    pub(super) fn stop_hard(&self) -> Result<(), VmwareError> {
        self.run_control("stop", Some("hard"))
    }

    pub(super) fn is_running(&self) -> Result<bool, VmwareError> {
        let output = run_query(&self.vmrun)?;
        if !output.status.success() {
            return Err(VmwareError::ControlFailed(output.status));
        }
        // A non-Unicode VMX path can never match vmrun's listing; failing
        // closed keeps release from reporting a running guest as stopped.
        let expected = self.vmx.to_str().ok_or(VmwareError::NonUnicodePath)?;
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.trim().eq_ignore_ascii_case(expected)))
    }

    pub(super) fn stop_bounded(&self) -> Result<(), VmwareError> {
        match self.stop_soft() {
            Ok(()) => self.confirm_soft_stop(),
            Err(soft_error) => {
                if !self.is_running()? {
                    return self.confirm_vmx_exit();
                }
                tracing::warn!(error = %soft_error, "VMware soft stop failed; forcing COW-backed VM off");
                self.stop_hard()?;
                self.confirm_vmx_exit()
            }
        }
    }

    /// `vmrun stop soft` reports success when the ACPI shutdown request was
    /// delivered, not when the guest actually powered off. Poll until the VM
    /// disappears from `vmrun list`; if it is still running past the confirm
    /// window, fall back to a hard stop so the COW overlay can be flushed.
    fn confirm_soft_stop(&self) -> Result<(), VmwareError> {
        let deadline = Instant::now() + SOFT_STOP_CONFIRM_TIMEOUT;
        loop {
            if !self.is_running()? {
                return self.confirm_vmx_exit();
            }
            if Instant::now() >= deadline {
                tracing::warn!(
                    "VMware guest still running after soft stop; forcing COW-backed VM off"
                );
                self.stop_hard()?;
                return self.confirm_vmx_exit();
            }
            thread::sleep(SOFT_STOP_POLL_INTERVAL);
        }
    }

    /// `vmrun list` can stop reporting a guest before VMX has finished
    /// flushing virtual disks and closing handles. The session workspace must
    /// remain intact until VMware writes its terminal `VMX exit` record.
    fn confirm_vmx_exit(&self) -> Result<(), VmwareError> {
        let deadline = Instant::now() + VMX_EXIT_CONFIRM_TIMEOUT;
        let log_path = self.vmx.parent().map(|path| path.join("vmware.log"));
        loop {
            if let Some(path) = log_path.as_deref() {
                match fs::read_to_string(path) {
                    Ok(contents) if vmx_log_has_exited_since(&contents, self.vmx_log_baseline) => {
                        return Ok(())
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(VmwareError::VmxLogRead(error)),
                }
            }
            if Instant::now() >= deadline {
                return Err(VmwareError::VmxExitTimeout {
                    timeout_secs: VMX_EXIT_CONFIRM_TIMEOUT.as_secs(),
                });
            }
            thread::sleep(VMX_EXIT_CONFIRM_INTERVAL);
        }
    }

    fn run_control(&self, action: &str, mode: Option<&str>) -> Result<(), VmwareError> {
        let mut command = Command::new(&self.vmrun);
        command
            .arg(action)
            .arg(&self.vmx)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(mode) = mode {
            command.arg(mode);
        }
        wait_for_control(
            command.spawn().map_err(VmwareError::Start)?,
            action,
            CONTROL_TIMEOUT,
        )
    }
}

pub(super) fn launch(vmx: &Path) -> Result<VmwareControl, VmwareError> {
    let (_workstation, vmrun) = discover()?;
    let vmx = vmware_compatible_path(vmx);
    let vmx_log_baseline = vmx
        .parent()
        .map(|path| path.join("vmware.log"))
        .and_then(|path| fs::metadata(path).ok().map(|metadata| metadata.len()));
    let child = Command::new(&vmrun)
        .arg("start")
        .arg(&vmx)
        .arg("gui")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(VmwareError::Start)?;
    let control = VmwareControl {
        vmrun,
        vmx,
        vmx_log_baseline,
    };
    match wait_for_control(child, "start", START_TIMEOUT) {
        Ok(()) => Ok(control),
        Err(error) => recover_after_start_failure(control, error),
    }
}

/// `vmrun start` can time out after it has already handed the VM to VMX. Use
/// the authoritative `vmrun list` state before deciding that startup failed;
/// this avoids stopping a guest that is already usable and avoids returning a
/// false timeout to the caller.
fn recover_after_start_failure(
    control: VmwareControl,
    start_error: VmwareError,
) -> Result<VmwareControl, VmwareError> {
    let deadline = Instant::now() + START_CONFIRM_TIMEOUT;
    loop {
        match control.is_running() {
            Ok(true) => {
                tracing::warn!(
                    error = %start_error,
                    "VMware start command ended without acknowledgement, but the guest is running"
                );
                return Ok(control);
            }
            Ok(false) if Instant::now() >= deadline => {
                if let Err(stop_error) = control.stop_hard() {
                    tracing::warn!(
                        error = %stop_error,
                        "VMware cleanup after an unconfirmed start timeout failed"
                    );
                }
                return Err(start_error);
            }
            Ok(false) => thread::sleep(START_CONFIRM_INTERVAL),
            Err(query_error) => {
                tracing::warn!(
                    error = %query_error,
                    "VMware guest state could not be confirmed after start failure"
                );
                if let Err(stop_error) = control.stop_hard() {
                    tracing::warn!(
                        error = %stop_error,
                        "VMware cleanup after unconfirmed start failed"
                    );
                }
                return Err(start_error);
            }
        }
    }
}

#[cfg(windows)]
fn vmware_compatible_path(path: &Path) -> PathBuf {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    const SLASH: u16 = b'\\' as u16;
    const VERBATIM: [u16; 4] = [SLASH, SLASH, b'?' as u16, SLASH];
    const VERBATIM_UNC: [u16; 8] = [
        SLASH,
        SLASH,
        b'?' as u16,
        SLASH,
        b'U' as u16,
        b'N' as u16,
        b'C' as u16,
        SLASH,
    ];

    let value = path.as_os_str().encode_wide().collect::<Vec<_>>();
    let normalized = if starts_with_ascii_case_insensitive(&value, &VERBATIM_UNC) {
        let mut output = vec![SLASH, SLASH];
        output.extend_from_slice(&value[VERBATIM_UNC.len()..]);
        output
    } else if value.starts_with(&VERBATIM)
        && value
            .get(4)
            .is_some_and(|unit| is_ascii_drive_letter(*unit))
        && value.get(5) == Some(&(b':' as u16))
        && value.get(6) == Some(&SLASH)
    {
        value[VERBATIM.len()..].to_vec()
    } else {
        return path.to_path_buf();
    };
    PathBuf::from(OsString::from_wide(&normalized))
}

#[cfg(windows)]
fn starts_with_ascii_case_insensitive(value: &[u16], prefix: &[u16]) -> bool {
    value.len() >= prefix.len()
        && value
            .iter()
            .zip(prefix)
            .all(|(left, right)| ascii_case_fold(*left) == ascii_case_fold(*right))
}

#[cfg(windows)]
fn is_ascii_drive_letter(value: u16) -> bool {
    value <= u8::MAX as u16 && (value as u8).is_ascii_alphabetic()
}

#[cfg(windows)]
fn ascii_case_fold(value: u16) -> u16 {
    if (b'A' as u16..=b'Z' as u16).contains(&value) {
        value + (b'a' - b'A') as u16
    } else {
        value
    }
}

#[cfg(not(windows))]
fn vmware_compatible_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

fn discover() -> Result<(PathBuf, PathBuf), VmwareError> {
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        let Some(root) = std::env::var_os(variable) else {
            continue;
        };
        for product in ["VMware Workstation", "VMware Player"] {
            let directory = PathBuf::from(&root).join("VMware").join(product);
            if let Some(pair) = probe_installation(&directory) {
                return Ok(pair);
            }
        }
    }
    #[cfg(windows)]
    for product in ["VMware Workstation", "VMware Player"] {
        if let Some(directory) = registry_install_path(product) {
            if let Some(pair) = probe_installation(&directory) {
                return Ok(pair);
            }
        }
    }
    Err(VmwareError::NotInstalled)
}

fn probe_installation(directory: &Path) -> Option<(PathBuf, PathBuf)> {
    let workstation = directory.join("vmware.exe");
    let vmrun = directory.join("vmrun.exe");
    (workstation.is_file() && vmrun.is_file()).then_some((workstation, vmrun))
}

/// Fallback discovery for machines where VMware Workstation or Player is not
/// installed under the default Program Files layout. Workstation is probed
/// before Player by the caller.
#[cfg(windows)]
fn registry_install_path(product: &str) -> Option<PathBuf> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    let subkey = HSTRING::from(format!(r"SOFTWARE\VMware, Inc.\{product}"));
    let value = HSTRING::from("InstallPath");
    let mut byte_len = 0u32;
    // SAFETY: all pointers reference valid NUL-terminated UTF-16 strings or
    // caller-owned buffers that outlive both calls; the first call sizes the
    // buffer and the second fills it.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut byte_len),
        )
    };
    if !status.is_ok() || byte_len == 0 {
        return None;
    }
    let mut buffer = vec![0u16; (byte_len as usize).div_ceil(2)];
    // SAFETY: `buffer` is sized from the first call and remains writable for
    // the duration of this call.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut byte_len),
        )
    };
    if !status.is_ok() {
        return None;
    }
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    let path = PathBuf::from(String::from_utf16_lossy(&buffer[..end]));
    path.is_dir().then_some(path)
}

fn wait_for_control(
    mut child: std::process::Child,
    operation: &str,
    timeout: Duration,
) -> Result<(), VmwareError> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().map_err(VmwareError::Start)? {
            return if status.success() {
                Ok(())
            } else {
                Err(VmwareError::ControlFailed(status))
            };
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(VmwareError::ControlTimeout {
                operation: operation.to_string(),
                timeout_secs: timeout.as_secs(),
            });
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn run_query(vmrun: &Path) -> Result<std::process::Output, VmwareError> {
    let child = Command::new(vmrun)
        .arg("list")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(VmwareError::Start)?;
    wait_for_output(child, "list", QUERY_TIMEOUT)
}

fn wait_for_output(
    mut child: std::process::Child,
    operation: &str,
    timeout: Duration,
) -> Result<std::process::Output, VmwareError> {
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait().map_err(VmwareError::Start)?.is_some() {
            return child.wait_with_output().map_err(VmwareError::Start);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(VmwareError::ControlTimeout {
                operation: operation.to_string(),
                timeout_secs: timeout.as_secs(),
            });
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn vmx_log_has_exited_since(contents: &str, baseline: Option<u64>) -> bool {
    let baseline = baseline.unwrap_or(0);
    let mut offset = 0u64;
    contents.lines().any(|line| {
        let folded = line.to_ascii_lowercase();
        let found = folded
            .find("vmx exit (")
            .and_then(|marker| u64::try_from(marker).ok())
            .and_then(|marker| offset.checked_add(marker))
            .is_some_and(|marker| marker >= baseline);
        offset = offset.saturating_add(u64::try_from(line.len()).unwrap_or(u64::MAX));
        // `lines` removes the newline; account for it when calculating the
        // byte offset of the next line in the original log.
        offset = offset.saturating_add(1);
        found
    })
}

#[cfg(test)]
#[path = "../../tests/unit/emulation_registry/vmware.rs"]
mod tests;
