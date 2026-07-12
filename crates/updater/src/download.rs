use std::path::{Path, PathBuf};

use sha2::Digest;
use tracing::info;

use super::UpdateError;

const APP_CODE_NAME: &str = "Meow_Detective";

pub async fn download_update(
    url: &str,
    expected_sha256: Option<&str>,
    current_version: &str,
) -> Result<PathBuf, UpdateError> {
    info!(url, "downloading update");
    let client = reqwest::Client::builder()
        .user_agent(format!("{APP_CODE_NAME}/{current_version}"))
        .build()
        .map_err(|error| UpdateError::TlsError(error.to_string()))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| UpdateError::DownloadError(error.to_string()))?;
    if !response.status().is_success() {
        return Err(UpdateError::DownloadError(format!(
            "HTTP {}",
            response.status()
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|error| UpdateError::DownloadError(error.to_string()))?;
    verify_hash(&bytes, expected_sha256)?;
    persist_installer(url, &bytes)
}

fn verify_hash(bytes: &[u8], expected_sha256: Option<&str>) -> Result<(), UpdateError> {
    let Some(expected) = expected_sha256 else {
        return Ok(());
    };
    let actual = hex::encode(sha2::Sha256::digest(bytes));
    if actual != expected {
        return Err(UpdateError::HashMismatch {
            expected: expected.to_string(),
            actual,
        });
    }
    info!("sha256 checksum verified");
    Ok(())
}

fn persist_installer(url: &str, bytes: &[u8]) -> Result<PathBuf, UpdateError> {
    let extension = url
        .split('?')
        .next()
        .and_then(|path| Path::new(path).extension())
        .and_then(|extension| extension.to_str())
        .unwrap_or("tmp");
    let temp_file = tempfile::Builder::new()
        .suffix(&format!(".{extension}"))
        .tempfile()
        .map_err(|error| UpdateError::DownloadError(error.to_string()))?;
    std::fs::write(temp_file.path(), bytes)
        .map_err(|error| UpdateError::DownloadError(error.to_string()))?;
    let (_, path) = temp_file
        .keep()
        .map_err(|error| UpdateError::DownloadError(error.to_string()))?;
    info!(path = %path.display(), "update downloaded");
    Ok(path)
}
