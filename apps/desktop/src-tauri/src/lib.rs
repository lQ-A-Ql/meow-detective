mod analysis_task_jobs;
mod app_update;
mod application_logging;
mod bitlocker_key_store;
mod cache_invalidation;
mod case_projection_jobs;
mod command_registry;
mod commands;
#[cfg(windows)]
mod dokan_runtime;
mod emulation_backend;
mod emulation_registry;
pub mod events;
mod evidence_task_jobs;
mod mcp_host;
mod mcp_report_task;
mod media_protocol;
mod mount_backend;
mod mount_registry;
mod physical_mount_registry;
mod platform_security;
mod post_import_scheduler;
pub mod state;

use command_registry::*;

#[macro_use]
mod command_handler;

mod desktop_runtime;
mod embedded_maintenance;
pub fn run() {
    let (logs, _log_guard) = application_logging::initialize();
    tracing::info!("Meow~Detective starting");
    if app_update::startup::recover_if_needed() {
        return;
    }
    let app = desktop_runtime::builder()
        .manage(app_services::application_log_service::ApplicationLogService::new(logs.clone()))
        .invoke_handler(desktop_command_handler!())
        .build(tauri::generate_context!())
        .unwrap_or_else(|error| {
            tracing::error!("Failed to run Tauri application: {error}");
            let _ = logs.shutdown();
            application_logging::show_failure(
                "Meow~Detective could not initialize its desktop window.",
            );
            std::process::exit(1);
        });
    desktop_runtime::run_app(app, logs);
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
