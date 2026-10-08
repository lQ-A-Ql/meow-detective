use super::super::ExtractionOutcome;
use crate::analysis_service::artifact_builders::{
    browser_attrs, make_artifact, string_array_value,
};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::error::AnalysisServiceError;
use artifacts_windows::browser::{parse_firefox_extensions, BrowserExtension};
use serde_json::Value;

pub(super) fn extract_firefox_extensions(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    profile: &str,
) -> Result<ExtractionOutcome, AnalysisServiceError> {
    let extensions = parse_firefox_extensions(bytes).map_err(AnalysisServiceError::Extraction)?;
    let mut outcome = ExtractionOutcome::default();
    for extension in extensions {
        outcome
            .artifacts
            .push(extension_artifact(candidate, profile, extension));
    }
    Ok(outcome)
}

fn extension_artifact(
    candidate: &EvidenceCandidate,
    profile: &str,
    extension: BrowserExtension,
) -> domain::Artifact {
    let mut attrs = browser_attrs(candidate, "Firefox", profile);
    attrs.insert(
        "sourceProfile".to_string(),
        Value::String(profile.to_string()),
    );
    attrs.insert("id".to_string(), Value::String(extension.id.clone()));
    attrs.insert("name".to_string(), Value::String(extension.name.clone()));
    attrs.insert(
        "version".to_string(),
        Value::String(extension.version.clone()),
    );
    attrs.insert("active".to_string(), Value::Bool(extension.active));
    attrs.insert(
        "userDisabled".to_string(),
        Value::Bool(extension.user_disabled),
    );
    if let Some(date) = extension.install_date {
        attrs.insert("installDate".to_string(), Value::String(date.to_rfc3339()));
    }
    if let Some(date) = extension.update_date {
        attrs.insert("updateDate".to_string(), Value::String(date.to_rfc3339()));
    }
    if let Some(state) = extension.signed_state {
        attrs.insert("signedState".to_string(), Value::String(state));
    }
    attrs.insert(
        "permissions".to_string(),
        string_array_value(&extension.permissions),
    );
    make_artifact(
        "BrowserExtension",
        format!(
            "Firefox extension: {}",
            if extension.name.is_empty() {
                &extension.id
            } else {
                &extension.name
            }
        ),
        extension.id,
        candidate,
        "browser.extensions",
        attrs,
    )
}
