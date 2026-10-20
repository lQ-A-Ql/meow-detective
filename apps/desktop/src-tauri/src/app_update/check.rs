use super::failure;
use crate::state::AppState;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;
use transport::{
    dto::{AppUpdateInfoDto, AppUpdateReleaseDto},
    CommandError,
};

pub(super) fn portable_root() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let root = exe.parent()?.to_path_buf();
    let inventory = portable_updater::read_metadata(&exe).ok()?;
    (inventory.format == 1 && inventory.version == env!("CARGO_PKG_VERSION")).then_some(root)
}

pub(crate) fn info(app: &AppHandle) -> Result<AppUpdateInfoDto, CommandError> {
    let state = app.state::<AppState>();
    let pending = state
        .app_update
        .pending
        .lock()
        .map_err(|_| failure("UPDATE_STATE"))?;
    Ok(AppUpdateInfoDto {
        current_version: app.package_info().version.to_string(),
        configured: cfg!(all(windows, target_arch = "x86_64"))
            && !portable_updater::PUBLIC_KEY.is_empty(),
        portable: portable_root().is_some(),
        available: pending.as_ref().map(|update| AppUpdateReleaseDto {
            version: update.version.clone(),
            notes: update
                .body
                .clone()
                .unwrap_or_default()
                .chars()
                .take(16_384)
                .collect(),
            schema_compatible: update
                .raw_json
                .get("case_schema")
                .and_then(|value| value.as_str())
                == Some(portable_updater::CASE_SCHEMA),
        }),
    })
}

pub(crate) async fn check(app: AppHandle) -> Result<AppUpdateInfoDto, CommandError> {
    let state = app.state::<AppState>();
    let _operation = state
        .app_update
        .operation
        .try_lock()
        .map_err(|_| failure("UPDATE_BUSY"))?;
    if !info(&app)?.configured {
        return info(&app);
    }
    let updater = app
        .updater_builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| failure("UPDATE_CONFIG"))?;
    let candidate = updater
        .check()
        .await
        .map_err(|_| failure("UPDATE_NETWORK"))?;
    if let Some(candidate) = &candidate {
        let expected = format!(
            "/lQ-A-Ql/meow-detective/releases/download/v{}/",
            candidate.version
        );
        let url = &candidate.download_url;
        if url.scheme() != "https"
            || url.host_str() != Some("github.com")
            || !url.path().starts_with(&expected)
            || !url.path().ends_with(".exe")
            || !url.username().is_empty()
            || url.password().is_some()
            || candidate
                .raw_json
                .get("package_format")
                .and_then(|value| value.as_u64())
                != Some(1)
        {
            return Err(failure("UPDATE_MANIFEST"));
        }
    }
    *state
        .app_update
        .pending
        .lock()
        .map_err(|_| failure("UPDATE_STATE"))? = candidate;
    info(&app)
}
