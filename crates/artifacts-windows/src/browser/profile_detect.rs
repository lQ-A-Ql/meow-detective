//! Browser profile auto-detection.
//!
//! Scans evidence file paths for known browser profile marker files
//! (`Local State` for Chrome/Edge, `profiles.ini` for Firefox) and returns
//! discovered profiles.  Detection is purely path-based -- it looks at the
//! directory structure of evidence files to infer which browser profiles
//! exist, without requiring file content.

/// A discovered browser profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserProfile {
    /// Browser family: "Chrome", "Edge", or "Firefox".
    pub browser: String,
    /// Human-readable profile name, e.g. "Default", "Profile 1".
    pub profile_name: String,
    /// Relative path to the profile directory in the evidence source.
    pub profile_path: String,
}

/// Scan a list of evidence file paths for browser profiles.
///
/// Looks for:
/// - `*/Google/Chrome/User Data/Local State` or `*/Google/Chrome/User Data/*/History`
///   for Chrome profiles.
/// - `*/Microsoft/Edge/User Data/Local State` or `*/Microsoft/Edge/User Data/*/History`
///   for Edge profiles.
/// - `*/Mozilla/Firefox/profiles.ini` or `*/Mozilla/Firefox/Profiles/*/places.sqlite`
///   for Firefox profiles.
///
/// Returns deduplicated profiles sorted by browser then profile name.
pub fn detect_browser_profiles(file_paths: &[String]) -> Vec<BrowserProfile> {
    let mut profiles: Vec<BrowserProfile> = Vec::new();

    for path in file_paths {
        let normalized = normalize_path(path);

        // ── Chrome ──────────────────────────────────────────────
        if let Some(base) = extract_base_dir(&normalized, "/google/chrome/user data/") {
            if let Some(profile_dir) = extract_chrome_profile_dir(&normalized, &base) {
                insert_if_new(
                    &mut profiles,
                    BrowserProfile {
                        browser: "Chrome".to_string(),
                        profile_name: chromelike_dir_to_name(&profile_dir),
                        profile_path: format_user_data_profile(&base, "Chrome", &profile_dir),
                    },
                );
            }
        }

        // ── Edge ────────────────────────────────────────────────
        if let Some(base) = extract_base_dir(&normalized, "/microsoft/edge/user data/") {
            if let Some(profile_dir) = extract_chrome_profile_dir(&normalized, &base) {
                insert_if_new(
                    &mut profiles,
                    BrowserProfile {
                        browser: "Edge".to_string(),
                        profile_name: chromelike_dir_to_name(&profile_dir),
                        profile_path: format_user_data_profile(&base, "Edge", &profile_dir),
                    },
                );
            }
        }

        // ── Firefox ─────────────────────────────────────────────
        if let Some(profile_dir) = extract_firefox_profile_dir(&normalized) {
            insert_if_new(
                &mut profiles,
                BrowserProfile {
                    browser: "Firefox".to_string(),
                    profile_name: firefox_dir_to_name(&profile_dir),
                    profile_path: profile_dir.clone(),
                },
            );
        }
    }

    // Deduplicate and sort for deterministic output.
    profiles.sort_by(|a, b| {
        a.browser
            .cmp(&b.browser)
            .then_with(|| a.profile_name.cmp(&b.profile_name))
    });
    profiles.dedup_by(|a, b| a.browser == b.browser && a.profile_path == b.profile_path);
    profiles
}

// ── helpers ──────────────────────────────────────────────────────────

/// Normalize a path to lower-case with forward slashes, always starting with `/`.
fn normalize_path(path: &str) -> String {
    let n = path.replace('\\', "/").to_ascii_lowercase();
    if n.starts_with('/') {
        n
    } else {
        format!("/{}", n)
    }
}

/// Extract the base directory (everything before the marker), including the
/// trailing `/` of the parent.  Returns `None` if the marker is not found.
fn extract_base_dir(normalized: &str, marker: &str) -> Option<String> {
    // We need to find a marker like `/google/chrome/user data/` somewhere
    // in the path, and return everything up to and including that segment.
    let pos = normalized.rfind(marker)?;
    Some(normalized[..=pos + marker.len() - 1].to_string())
}

/// Given a base like `/.../google/chrome/user data/` and the current
/// normalized path, extract the Chromium-like profile directory name
/// (e.g. `default`, `profile 1`).  Only returns a value when the path
/// points to a known browser database file.
fn extract_chrome_profile_dir(normalized: &str, base: &str) -> Option<String> {
    let remainder = normalized.strip_prefix(base)?;
    // remainder is now e.g. "default/history" or "profile 1/archived history"
    let first_component = remainder.trim_start_matches('/').split('/').next()?;
    if first_component.is_empty() {
        return None;
    }
    // Must be a known Chromium history database file further down.
    let after_profile = &remainder[first_component.len()..].trim_start_matches('/');
    if after_profile.eq_ignore_ascii_case("history")
        || after_profile.eq_ignore_ascii_case("archived history")
        || after_profile.eq_ignore_ascii_case("cookies")
        || after_profile.starts_with("local state")
        || after_profile.starts_with("last session")
        || after_profile.starts_with("current session")
    {
        Some(first_component.to_string())
    } else {
        None
    }
}

/// Convert a Chromium-like profile directory name to a human-readable name.
fn chromelike_dir_to_name(dir: &str) -> String {
    // Capitalise first letter of each word: separators are space, dash, or underscore.
    // Underscores become spaces in the output.
    let mut result = String::with_capacity(dir.len());
    let mut capitalise = true;
    for ch in dir.chars() {
        if ch == ' ' || ch == '-' {
            result.push(ch);
            capitalise = true;
        } else if ch == '_' {
            result.push(' ');
            capitalise = true;
        } else if capitalise {
            result.extend(ch.to_uppercase());
            capitalise = false;
        } else {
            result.push(ch);
        }
    }
    result
}

/// Build a human-friendly profile path relative to the evidence root.
fn format_user_data_profile(base: &str, browser: &str, profile_dir: &str) -> String {
    // base ends with something like "/google/chrome/user data/"
    // Strip the trailing browser-specific segments and re-append with capitalisation.
    let marker = if browser == "Edge" {
        "/microsoft/edge/user data/"
    } else {
        "/google/chrome/user data/"
    };
    // For the path, use the original base but with a clean representation.
    if let Some(prefix_end) = base.rfind(marker) {
        let prefix = &base[..prefix_end];
        format!("{}{}{}", prefix, marker, profile_dir)
    } else {
        format!("{}{}", base, profile_dir)
    }
}

/// Extract a Firefox profile directory from a path like
/// `/.../mozilla/firefox/profiles/abc123.default-release/places.sqlite`.
fn extract_firefox_profile_dir(normalized: &str) -> Option<String> {
    let marker = "/mozilla/firefox/profiles/";
    let pos = normalized.find(marker)?;
    let after = &normalized[pos + marker.len()..];
    let profile_dir = after.split('/').next()?;
    if profile_dir.is_empty() {
        return None;
    }
    Some(format!("/mozilla/firefox/profiles/{}", profile_dir))
}

/// Derive a short human-readable profile name from a Firefox profile directory.
fn firefox_dir_to_name(dir: &str) -> String {
    // The directory is typically something like "abc123.default-release"
    // or "xyz789.default".  Extract the descriptive suffix.
    let base = dir.rsplit('/').next().unwrap_or(dir);
    // If it has a dot, take everything after the first dot as the type.
    if let Some(suffix) = base.split('.').nth(1) {
        if !suffix.is_empty() {
            return suffix.to_string();
        }
    }
    base.to_string()
}

/// Insert a profile if not already present (matching by browser + profile_path).
fn insert_if_new(profiles: &mut Vec<BrowserProfile>, profile: BrowserProfile) {
    if !profiles.iter().any(|existing| {
        existing.browser == profile.browser && existing.profile_path == profile.profile_path
    }) {
        profiles.push(profile);
    }
}

#[cfg(test)]
#[path = "../../tests/unit/browser_profile_detect.rs"]
mod tests;
