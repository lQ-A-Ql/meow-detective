use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;

const CONTROL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Error)]
pub(super) enum VmwareError {
    #[error("VMware Workstation and vmrun were not found in a supported installation directory")]
    NotInstalled,
    #[error("VMware process could not be started: {0}")]
    Start(std::io::Error),
    #[error("VMware control command timed out")]
    ControlTimeout,
    #[error("VMware control command failed with status {0}")]
    ControlFailed(ExitStatus),
}

#[derive(Clone)]
pub(super) struct VmwareControl {
    vmrun: PathBuf,
    vmx: PathBuf,
}

impl VmwareControl {
    pub(super) fn stop_soft(&self) -> Result<(), VmwareError> {
        self.run_control("stop", Some("soft"))
    }

    pub(super) fn stop_hard(&self) -> Result<(), VmwareError> {
        self.run_control("stop", Some("hard"))
    }

    pub(super) fn is_running(&self) -> Result<bool, VmwareError> {
        let output = Command::new(&self.vmrun)
            .arg("list")
            .stdin(Stdio::null())
            .output()
            .map_err(VmwareError::Start)?;
        if !output.status.success() {
            return Err(VmwareError::ControlFailed(output.status));
        }
        let expected = self.vmx.to_string_lossy();
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.trim().eq_ignore_ascii_case(&expected)))
    }

    pub(super) fn stop_bounded(&self) -> Result<(), VmwareError> {
        match self.stop_soft() {
            Ok(()) => Ok(()),
            Err(soft_error) => {
                if !self.is_running()? {
                    return Ok(());
                }
                tracing::warn!(error = %soft_error, "VMware soft stop failed; forcing COW-backed VM off");
                self.stop_hard()
            }
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
        wait_for_control(command.spawn().map_err(VmwareError::Start)?)
    }
}

pub(super) fn launch(vmx: &Path) -> Result<VmwareControl, VmwareError> {
    let (_workstation, vmrun) = discover()?;
    let vmx = vmware_compatible_path(vmx);
    let child = Command::new(&vmrun)
        .arg("start")
        .arg(&vmx)
        .arg("gui")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(VmwareError::Start)?;
    wait_for_control(child)?;
    Ok(VmwareControl { vmrun, vmx })
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
        let directory = PathBuf::from(root)
            .join("VMware")
            .join("VMware Workstation");
        if let Some(pair) = probe_installation(&directory) {
            return Ok(pair);
        }
    }
    #[cfg(windows)]
    if let Some(directory) = registry_install_path() {
        if let Some(pair) = probe_installation(&directory) {
            return Ok(pair);
        }
    }
    Err(VmwareError::NotInstalled)
}

fn probe_installation(directory: &Path) -> Option<(PathBuf, PathBuf)> {
    let workstation = directory.join("vmware.exe");
    let vmrun = directory.join("vmrun.exe");
    (workstation.is_file() && vmrun.is_file()).then_some((workstation, vmrun))
}

/// Fallback discovery for machines where VMware Workstation is not installed
/// under the default Program Files layout.
#[cfg(windows)]
fn registry_install_path() -> Option<PathBuf> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    let subkey = HSTRING::from(r"SOFTWARE\VMware, Inc.\VMware Workstation");
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

fn wait_for_control(mut child: std::process::Child) -> Result<(), VmwareError> {
    let deadline = Instant::now() + CONTROL_TIMEOUT;
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
            return Err(VmwareError::ControlTimeout);
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
#[path = "../../tests/unit/emulation_registry/vmware.rs"]
mod tests;
