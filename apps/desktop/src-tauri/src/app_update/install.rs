use super::{check::portable_root, download::download, failure, handoff};
use crate::state::AppState;
use tauri::{ipc::Channel, AppHandle, Manager};
use transport::{dto::AppUpdateProgressDto, CommandError};

pub(crate) async fn install(
    app: AppHandle,
    version: String,
    progress: Channel<AppUpdateProgressDto>,
) -> Result<(), CommandError> {
    let state = app.state::<AppState>();
    let _operation = state
        .app_update
        .operation
        .try_lock()
        .map_err(|_| failure("UPDATE_BUSY"))?;
    let _lifecycle = state
        .case_lifecycle
        .try_write()
        .map_err(|_| failure("UPDATE_CASE_BUSY"))?;
    if state
        .active_case
        .lock()
        .map_err(|_| failure("UPDATE_STATE"))?
        .is_some()
    {
        return Err(failure("UPDATE_CLOSE_CASE"));
    }
    let task_guard = state
        .task_manager
        .begin_update()
        .ok_or_else(|| failure("UPDATE_TASKS_BUSY"))?;
    if !state
        .mount_registry
        .list()
        .map_err(|_| failure("UPDATE_STATE"))?
        .is_empty()
        || !state
            .physical_mount_registry
            .list()
            .map_err(|_| failure("UPDATE_STATE"))?
            .is_empty()
        || !state
            .emulation_registry
            .list()
            .map_err(|_| failure("UPDATE_STATE"))?
            .is_empty()
    {
        return Err(failure("UPDATE_MOUNTS_BUSY"));
    }
    let root = portable_root().ok_or_else(|| failure("UPDATE_PORTABLE_REQUIRED"))?;
    let update = state
        .app_update
        .pending
        .lock()
        .map_err(|_| failure("UPDATE_STATE"))?
        .as_ref()
        .filter(|update| update.version == version)
        .cloned()
        .ok_or_else(|| failure("UPDATE_STALE"))?;
    if update
        .raw_json
        .get("case_schema")
        .and_then(|value| value.as_str())
        != Some(portable_updater::CASE_SCHEMA)
    {
        return Err(failure("UPDATE_SCHEMA_MANUAL"));
    }
    let workspace = root.join(".meow-update");
    // Never reuse or recursively remove an unknown pre-existing directory.
    if workspace.exists() {
        portable_updater::clean_workspace(&root).map_err(|_| failure("UPDATE_PENDING_RECOVERY"))?;
    }
    std::fs::create_dir(&workspace).map_err(|_| failure("UPDATE_PENDING_RECOVERY"))?;
    let result = prepare(&root, &update, &progress).await;
    if result.is_err() {
        cleanup_failed_download(&workspace);
    }
    result?;
    let _ = progress.send(AppUpdateProgressDto {
        phase: "restarting".into(),
        downloaded_bytes: 0,
        total_bytes: None,
    });
    // Do not reopen task admission between helper acknowledgement and process exit.
    std::mem::forget(task_guard);
    app.exit(0);
    Ok(())
}

async fn prepare(
    root: &std::path::Path,
    update: &tauri_plugin_updater::Update,
    progress: &Channel<AppUpdateProgressDto>,
) -> Result<(), CommandError> {
    let workspace = root.join(".meow-update");
    let archive = workspace.join("update.exe");
    download(update, &archive, progress).await?;
    let signature = update.signature.clone();
    let verification_path = archive.clone();
    let candidate_version = update.version.clone();
    tauri::async_runtime::spawn_blocking(move || {
        portable_updater::validate_candidate(
            &verification_path,
            &signature,
            portable_updater::PUBLIC_KEY,
            &candidate_version,
            portable_updater::CASE_SCHEMA,
        )
    })
    .await
    .map_err(|_| failure("UPDATE_VERIFY"))?
    .map_err(|_| failure("UPDATE_SIGNATURE"))?;
    std::fs::write(workspace.join("update.sig"), &update.signature)
        .map_err(|_| failure("UPDATE_STORAGE"))?;
    let root = root.to_owned();
    let version = update.version.clone();
    tauri::async_runtime::spawn_blocking(move || handoff::launch(&root, "apply", &version))
        .await
        .map_err(|_| failure("UPDATE_HELPER"))?
}

fn cleanup_failed_download(workspace: &std::path::Path) {
    // Only remove files this code created. An extraction or rollback error remains reviewable.
    if workspace.join("prepared").exists() {
        return;
    }
    for name in ["update.exe", "update.sig", "error.txt", "guard", "ready"] {
        let _ = std::fs::remove_file(workspace.join(name));
    }
    let _ = std::fs::remove_dir(workspace);
}
