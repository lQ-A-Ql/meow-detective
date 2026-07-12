use tracing::{debug, info};

use super::{version::is_newer, UpdateError, UpdateManifest};

const APP_CODE_NAME: &str = "Meow_Detective";

pub async fn check_for_update(
    current_version: &str,
    update_endpoint: &str,
) -> Result<Option<UpdateManifest>, UpdateError> {
    debug!(current_version, update_endpoint, "checking for update");
    let client = reqwest::Client::builder()
        .user_agent(format!("{APP_CODE_NAME}/{current_version}"))
        .build()
        .map_err(|error| UpdateError::TlsError(error.to_string()))?;
    let response = client
        .get(update_endpoint)
        .send()
        .await
        .map_err(|error| UpdateError::FetchError(error.to_string()))?;
    if !response.status().is_success() {
        return Err(UpdateError::FetchError(format!(
            "HTTP {}",
            response.status()
        )));
    }

    let manifest: UpdateManifest = response
        .json()
        .await
        .map_err(|error| UpdateError::FetchError(error.to_string()))?;
    if !is_newer(&manifest.version, current_version) {
        info!(latest = %manifest.version, current = %current_version, "already up to date");
        return Ok(None);
    }

    info!(latest = %manifest.version, current = %current_version, "update available");
    Ok(Some(manifest))
}
