use super::failure;
use std::{io::Write, path::Path, time::Duration};
use tauri::ipc::Channel;
use transport::{dto::AppUpdateProgressDto, CommandError};

pub(super) async fn download(
    update: &tauri_plugin_updater::Update,
    path: &Path,
    progress: &Channel<AppUpdateProgressDto>,
) -> Result<(), CommandError> {
    let client = reqwest::Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|_| failure("UPDATE_NETWORK"))?;
    let mut response = client
        .get(update.download_url.clone())
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|_| failure("UPDATE_NETWORK"))?;
    let total = response.content_length();
    if total.is_some_and(|size| size > portable_updater::MAX_ARCHIVE_BYTES) {
        return Err(failure("UPDATE_TOO_LARGE"));
    }
    let mut file = std::fs::File::options()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| failure("UPDATE_STORAGE"))?;
    let mut downloaded = 0u64;
    let mut last_progress = std::time::Instant::now();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| failure("UPDATE_NETWORK"))?
    {
        downloaded += chunk.len() as u64;
        if downloaded > portable_updater::MAX_ARCHIVE_BYTES {
            return Err(failure("UPDATE_TOO_LARGE"));
        }
        file.write_all(&chunk)
            .map_err(|_| failure("UPDATE_STORAGE"))?;
        if last_progress.elapsed() >= Duration::from_millis(100) {
            let _ = progress.send(AppUpdateProgressDto {
                phase: "downloading".into(),
                downloaded_bytes: downloaded,
                total_bytes: total,
            });
            last_progress = std::time::Instant::now();
        }
    }
    file.sync_all().map_err(|_| failure("UPDATE_STORAGE"))?;
    let _ = progress.send(AppUpdateProgressDto {
        phase: "verifying".into(),
        downloaded_bytes: downloaded,
        total_bytes: total,
    });
    Ok(())
}
