use super::failure;
use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
use transport::CommandError;

pub(super) fn launch(root: &Path, mode: &str, version: &str) -> Result<(), CommandError> {
    let helper = std::env::current_exe().map_err(|_| failure("UPDATE_HELPER"))?;
    portable_updater::reject_links(&helper).map_err(|_| failure("UPDATE_HELPER"))?;
    let temporary = tempfile::Builder::new()
        .prefix("meow-update-helper-")
        .tempdir()
        .map_err(|_| failure("UPDATE_STORAGE"))?;
    let executable = temporary.path().join("meow-portable-updater.exe");
    std::fs::copy(helper, &executable).map_err(|_| failure("UPDATE_HELPER"))?;
    let mut command = Command::new(executable);
    command
        .arg("--meow-portable-update")
        .arg(mode)
        .arg(std::env::current_exe().map_err(|_| failure("UPDATE_HELPER"))?)
        .arg(std::process::id().to_string())
        .arg(version);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|_| failure("UPDATE_HELPER"))?;
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(30) {
        if root.join(".meow-update/ready").is_file() {
            // The running helper lives outside the program directory being replaced.
            let _ = temporary.keep();
            return Ok(());
        }
        if child
            .try_wait()
            .map_err(|_| failure("UPDATE_HELPER"))?
            .is_some()
        {
            return Err(failure("UPDATE_HELPER"));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    Err(failure("UPDATE_HELPER_TIMEOUT"))
}
