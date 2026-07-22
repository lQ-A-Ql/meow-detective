use super::ExtractionOutcome;
use crate::analysis_service::artifact_builders::{browser_attrs, make_artifact};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::error::AnalysisServiceError;
use artifacts_windows::browser::{
    parse_chrome_cookies_with_decryptor, parse_chrome_passwords_with_decryptor,
    parse_chrome_session, parse_firefox_cookies, parse_firefox_passwords, parse_firefox_session,
    BrowserCookie, BrowserPassword, BrowserSessionTab,
};
use serde_json::Value;

pub(super) fn extract_browser_cookies(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    browser: &str,
    profile: &str,
    decryptor: Option<&artifacts_windows::dpapi::ChromiumDecryptor>,
) -> Result<ExtractionOutcome, AnalysisServiceError> {
    let mut outcome = ExtractionOutcome::default();
    let cookies: Vec<BrowserCookie> = if browser == "Firefox" {
        parse_firefox_cookies(bytes).unwrap_or_default()
    } else {
        parse_chrome_cookies_with_decryptor(bytes, browser, Some(profile), decryptor)
            .map_err(AnalysisServiceError::Extraction)?
    };
    for cookie in cookies {
        let mut attrs = browser_attrs(candidate, browser, profile);
        attrs.insert("domain".to_string(), Value::String(cookie.domain.clone()));
        attrs.insert("name".to_string(), Value::String(cookie.name.clone()));
        if let Some(preview) = &cookie.value_preview {
            attrs.insert("valuePreview".to_string(), Value::String(preview.clone()));
        }
        attrs.insert(
            "decryptionStatus".to_string(),
            Value::String(cookie.decryption_status.as_str().to_string()),
        );
        if let Some(detail) = &cookie.decryption_detail {
            attrs.insert(
                "decryptionDetail".to_string(),
                Value::String(detail.clone()),
            );
        }
        if let Some(expiry) = cookie.expiry {
            attrs.insert("expiry".to_string(), Value::String(expiry.to_rfc3339()));
        }
        attrs.insert("secure".to_string(), Value::Bool(cookie.secure));
        attrs.insert("httpOnly".to_string(), Value::Bool(cookie.http_only));
        if let Some(same_site) = cookie.same_site {
            attrs.insert("sameSite".to_string(), Value::Number(same_site.into()));
        }
        outcome.artifacts.push(make_artifact(
            "BrowserCookie",
            format!("{} cookie: {}@{}", browser, cookie.name, cookie.domain),
            cookie.domain.clone(),
            candidate,
            "browser.history",
            attrs,
        ));
    }
    Ok(outcome)
}

pub(super) fn extract_browser_passwords(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    browser: &str,
    profile: &str,
    decryptor: Option<&artifacts_windows::dpapi::ChromiumDecryptor>,
) -> Result<ExtractionOutcome, AnalysisServiceError> {
    let mut outcome = ExtractionOutcome::default();
    let passwords: Vec<BrowserPassword> = if browser == "Firefox" {
        parse_firefox_passwords(bytes).unwrap_or_default()
    } else {
        parse_chrome_passwords_with_decryptor(bytes, browser, Some(profile), decryptor)
            .map_err(AnalysisServiceError::Extraction)?
    };
    for password in passwords {
        let mut attrs = browser_attrs(candidate, browser, profile);
        attrs.insert("url".to_string(), Value::String(password.url.clone()));
        attrs.insert(
            "username".to_string(),
            Value::String(password.username.clone()),
        );
        if let Some(preview) = &password.password_preview {
            attrs.insert(
                "passwordPreview".to_string(),
                Value::String(preview.clone()),
            );
        }
        attrs.insert(
            "decryptionStatus".to_string(),
            Value::String(password.decryption_status.as_str().to_string()),
        );
        if let Some(detail) = &password.decryption_detail {
            attrs.insert(
                "decryptionDetail".to_string(),
                Value::String(detail.clone()),
            );
        }
        if let Some(created_at) = password.created_at {
            attrs.insert(
                "createdAt".to_string(),
                Value::String(created_at.to_rfc3339()),
            );
        }
        attrs.insert(
            "timesUsed".to_string(),
            Value::Number(serde_json::Number::from(password.times_used)),
        );
        outcome.artifacts.push(make_artifact(
            "BrowserPassword",
            format!("{} password: {}", browser, password.url),
            password.url.clone(),
            candidate,
            "browser.history",
            attrs,
        ));
    }
    Ok(outcome)
}

pub(super) fn extract_browser_sessions(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    browser: &str,
    profile: &str,
) -> Result<ExtractionOutcome, AnalysisServiceError> {
    let mut outcome = ExtractionOutcome::default();
    let sessions: Vec<BrowserSessionTab> = if browser == "Firefox" {
        parse_firefox_session(bytes).unwrap_or_default()
    } else {
        parse_chrome_session(bytes).unwrap_or_default()
    };
    for session in sessions {
        let mut attrs = browser_attrs(candidate, browser, profile);
        attrs.insert("url".to_string(), Value::String(session.url.clone()));
        if let Some(title) = &session.title {
            attrs.insert("title".to_string(), Value::String(title.clone()));
        }
        attrs.insert(
            "windowIndex".to_string(),
            Value::Number(session.window_index.into()),
        );
        attrs.insert(
            "tabIndex".to_string(),
            Value::Number(session.tab_index.into()),
        );
        if let Some(last_active) = session.last_active {
            attrs.insert(
                "lastActive".to_string(),
                Value::String(last_active.to_rfc3339()),
            );
        }
        outcome.artifacts.push(make_artifact(
            "BrowserSessionTab",
            format!(
                "{} session: {}",
                browser,
                session.title.as_deref().unwrap_or(&session.url)
            ),
            session.url.clone(),
            candidate,
            "browser.history",
            attrs,
        ));
    }
    Ok(outcome)
}
