pub(crate) fn capture_sanitized_backtrace() -> Vec<String> {
    let backtrace = std::backtrace::Backtrace::capture();
    let status = backtrace.status();
    if status == std::backtrace::BacktraceStatus::Captured {
        return format!("{backtrace:#?}")
            .lines()
            .filter(|line| line.contains("forensic") || line.contains("evidence"))
            .map(sanitize_path)
            .collect();
    }

    vec![format!(
        "backtrace unavailable (status = {status:?}). Set RUST_BACKTRACE=1 for a full trace."
    )]
}

pub(crate) fn sanitize_path(raw: &str) -> String {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    let sanitized = if home.is_empty() {
        raw.to_string()
    } else {
        raw.replace(&home, "~")
    };
    if sanitized.starts_with("\\\\?\\") {
        "<long-path>".to_string()
    } else {
        sanitized
    }
}
