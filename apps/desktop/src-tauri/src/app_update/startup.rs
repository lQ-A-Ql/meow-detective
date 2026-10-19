use super::handoff;

fn program_root() -> Option<std::path::PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.to_owned())
}

pub(crate) fn recover_if_needed() -> bool {
    if std::env::args_os().any(|argument| argument == "--meow-update-boot") {
        return false;
    }
    let Some(root) = program_root() else {
        return false;
    };
    if !portable_updater::rollback_pending(&root) {
        return false;
    }
    if let Ok(guard) = std::fs::File::open(root.join(".meow-update/guard")) {
        if guard.try_lock().is_err() {
            return true;
        }
    }
    let _ = std::fs::remove_file(root.join(".meow-update/ready"));
    if let Err(error) = handoff::launch(&root, "recover", env!("CARGO_PKG_VERSION")) {
        tracing::error!(
            code = error.code,
            "Portable update recovery could not start"
        );
    }
    // A pending transaction must be recovered before any case can open.
    true
}

pub(crate) fn acknowledge_boot() {
    if !std::env::args_os().any(|argument| argument == "--meow-update-boot") {
        return;
    }
    if let Some(root) = program_root() {
        let _ = std::fs::write(root.join(".meow-update/booted"), b"booted");
    }
}
