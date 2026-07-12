use chrono::{DateTime, TimeZone, Utc};

pub(super) fn browser_profile_from_path(normalized: &str) -> (String, String) {
    let browser = if normalized.contains("/microsoft/edge/user data/") {
        "Edge"
    } else if normalized.contains("/mozilla/firefox/profiles/") {
        "Firefox"
    } else {
        "Chrome"
    };
    let marker = if browser == "Firefox" {
        "/mozilla/firefox/profiles/"
    } else if browser == "Edge" {
        "/microsoft/edge/user data/"
    } else {
        "/google/chrome/user data/"
    };
    let raw_profile = normalized
        .split_once(marker)
        .map(|(_, rest)| rest.split('/').next().unwrap_or("default"))
        .filter(|value| !value.is_empty())
        .unwrap_or("default");

    let profile = if browser == "Firefox" {
        raw_profile.to_string()
    } else {
        capitalise_words(raw_profile)
    };
    (browser.to_string(), profile)
}

fn capitalise_words(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut capitalise = true;
    for ch in input.chars() {
        if ch == ' ' || ch == '-' || ch == '_' {
            result.push(ch);
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

pub(super) fn chromium_time_to_dt(value: i64) -> Option<DateTime<Utc>> {
    if value <= 0 {
        return None;
    }
    let seconds = value / 1_000_000 - 11_644_473_600;
    let nanos = ((value % 1_000_000) * 1_000) as u32;
    Utc.timestamp_opt(seconds, nanos).single()
}

pub(super) fn unix_microseconds_to_dt(value: i64) -> Option<DateTime<Utc>> {
    if value <= 0 {
        return None;
    }
    Utc.timestamp_opt(value / 1_000_000, ((value % 1_000_000) * 1_000) as u32)
        .single()
}
