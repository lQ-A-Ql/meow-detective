use std::time::Duration;

pub(super) fn profile_value(detail: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    detail.split_whitespace().find_map(|part| {
        part.strip_prefix(&prefix)
            .map(|value| value.trim_end_matches([',', ';']).to_string())
    })
}

pub(super) fn profile_u64(detail: &str, key: &str) -> Option<u64> {
    profile_value(detail, key).and_then(|value| value.parse::<u64>().ok())
}

pub(super) fn profile_nonzero_u64(detail: &str, key: &str) -> Option<u64> {
    profile_u64(detail, key).filter(|value| *value > 0)
}

pub(super) fn profile_f64(detail: &str, key: &str) -> Option<f64> {
    profile_value(detail, key).and_then(|value| value.parse::<f64>().ok())
}

pub(super) fn rows_from_profile(detail: &str) -> (u64, Option<u64>) {
    if let Some(processed) = profile_value(detail, "processed") {
        if let Some((done, total)) = processed.split_once('/') {
            return (done.parse::<u64>().unwrap_or(0), total.parse::<u64>().ok());
        }
        if let Ok(rows) = processed.parse::<u64>() {
            return (rows, profile_u64(detail, "files"));
        }
    }
    let rows = profile_u64(detail, "rows").unwrap_or(0);
    (
        rows,
        profile_u64(detail, "files").or_else(|| profile_u64(detail, "pendingTasks")),
    )
}

pub(crate) fn elapsed_ms(duration: Duration) -> u128 {
    duration.as_millis()
}

pub(crate) fn rows_per_sec(rows: u64, duration: Duration) -> u64 {
    let secs = duration.as_secs_f64();
    if secs <= 0.0 {
        rows
    } else {
        (rows as f64 / secs).round() as u64
    }
}

pub(crate) fn bytes_to_mb(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

pub(crate) fn mb_per_sec(bytes: u64, duration: Duration) -> u64 {
    let secs = duration.as_secs_f64();
    if secs <= 0.0 {
        bytes_to_mb(bytes)
    } else {
        ((bytes as f64 / (1024.0 * 1024.0)) / secs).round() as u64
    }
}
