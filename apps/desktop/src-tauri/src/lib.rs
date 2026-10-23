mod analysis_task_jobs;
mod app_update;
mod bitlocker_key_store;
mod cache_invalidation;
mod command_registry;
mod commands;
#[cfg(windows)]
mod dokan_runtime;
mod emulation_backend;
mod emulation_registry;
pub mod events;
mod evidence_task_jobs;
mod mcp_host;
mod media_protocol;
mod mount_backend;
mod mount_registry;
mod physical_mount_registry;
mod platform_security;
pub mod state;

use command_registry::*;

#[macro_use]
mod command_handler;

mod desktop_runtime;
mod embedded_maintenance;
pub fn run() {
    if app_update::startup::recover_if_needed() {
        return;
    }
    let app = desktop_runtime::builder()
        .invoke_handler(desktop_command_handler!())
        .build(tauri::generate_context!())
        .unwrap_or_else(|error| {
            tracing::error!("Failed to run Tauri application: {error}");
            std::process::exit(1);
        });
    desktop_runtime::run_app(app);
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
