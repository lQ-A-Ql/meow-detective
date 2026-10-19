use crate::{
    apply, process::Parent, read_metadata, recover, validate_candidate, UpdateError, CASE_SCHEMA,
    PUBLIC_KEY,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn run_if_requested() -> bool {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments
        .first()
        .is_none_or(|argument| argument != "--meow-portable-update")
    {
        return false;
    }
    if let Err(error) = execute(&arguments) {
        if let Some(target) = arguments.get(2).map(PathBuf::from) {
            if let Some(root) = target.parent() {
                let _ = fs::write(root.join(".meow-update/error.txt"), error.to_string());
            }
        }
    }
    true
}

fn execute(arguments: &[std::ffi::OsString]) -> Result<(), UpdateError> {
    if arguments.len() != 5 {
        return Err(UpdateError::Invalid("invalid helper arguments"));
    }
    let mode = arguments[1]
        .to_str()
        .ok_or(UpdateError::Invalid("invalid mode"))?;
    let target = PathBuf::from(&arguments[2]).canonicalize()?;
    let root = target
        .parent()
        .ok_or(UpdateError::Invalid("missing program directory"))?;
    let workspace = root.join(".meow-update");
    crate::reject_links(&workspace)?;
    let guard = fs::File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(workspace.join("guard"))?;
    guard.try_lock().map_err(|_| UpdateError::PendingRecovery)?;
    let pid = arguments[3]
        .to_str()
        .and_then(|value| value.parse().ok())
        .ok_or(UpdateError::Invalid("invalid parent PID"))?;
    let version = arguments[4]
        .to_str()
        .ok_or(UpdateError::Invalid("invalid version"))?;
    let parent = Parent::open(pid)?;
    if mode == "apply" {
        let signature = fs::read_to_string(workspace.join("update.sig"))?;
        let current = read_metadata(&target)?;
        let next =
            semver::Version::parse(version).map_err(|_| UpdateError::Invalid("invalid version"))?;
        let previous = semver::Version::parse(&current.version)
            .map_err(|_| UpdateError::Invalid("invalid installed version"))?;
        if next <= previous || current.case_schema != CASE_SCHEMA {
            return Err(UpdateError::Invalid("downgrade or incompatible schema"));
        }
        validate_candidate(
            &workspace.join("update.exe"),
            &signature,
            PUBLIC_KEY,
            version,
            CASE_SCHEMA,
        )?;
    } else if mode != "recover" {
        return Err(UpdateError::Invalid("unknown helper mode"));
    }
    fs::write(workspace.join("ready"), b"ready")?;
    parent.wait()?;
    if mode == "recover" {
        recover(&target)?;
    } else if let Err(error) = apply(&target, &workspace.join("update.exe")) {
        let _ = Command::new(&target).current_dir(root).spawn();
        return Err(error);
    }
    let boot = launch_and_confirm(&target, &workspace);
    if boot.is_err() {
        recover(&target)?;
        Command::new(&target).current_dir(root).spawn()?;
        return boot;
    }
    if mode == "apply" {
        fs::remove_file(workspace.join("prepared"))?;
    }
    drop(guard);
    crate::transaction::clean_workspace(root)?;
    Ok(())
}

fn launch_and_confirm(target: &Path, workspace: &Path) -> Result<(), UpdateError> {
    let acknowledgement = workspace.join("booted");
    let _ = fs::remove_file(&acknowledgement);
    let mut child = Command::new(target)
        .arg("--meow-update-boot")
        .current_dir(
            target
                .parent()
                .ok_or(UpdateError::Invalid("missing directory"))?,
        )
        .spawn()?;
    for _ in 0..300 {
        if acknowledgement.is_file() {
            return Ok(());
        }
        if child.try_wait()?.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    Err(UpdateError::Invalid(
        "new application did not acknowledge startup",
    ))
}
