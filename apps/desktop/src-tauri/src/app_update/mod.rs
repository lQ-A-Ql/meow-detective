mod check;
mod download;
mod handoff;
mod install;
pub(crate) mod startup;
pub(crate) use check::{check, info};
pub(crate) use install::install;

#[derive(Default)]
pub(crate) struct UpdateRuntime {
    operation: tokio::sync::Mutex<()>,
    pending: std::sync::Mutex<Option<tauri_plugin_updater::Update>>,
}

pub(super) fn failure(code: &str) -> transport::CommandError {
    let mut error = transport::CommandError::conflict("Application update could not complete; the current version is preserved. Retry or use the manual download.");
    error.code = code.to_owned();
    error
}
