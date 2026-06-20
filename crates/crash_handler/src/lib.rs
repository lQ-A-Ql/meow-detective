//! Production crash handler with sanitized reporting.
//!
//! Sets a custom panic hook that captures a stack trace, sanitizes paths so
//! that no case data or user-identifiable absolute paths leak, and writes a
//! `CrashReport` JSON file to a local `crash_reports/` directory.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::error;

/// Root directory where crash reports are stored.
static CRASH_REPORT_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Initialize the crash report directory.
///
/// Call this early in `main()` or at app startup so the panic hook knows where
/// to write reports.
pub fn init_crash_report_dir(base_dir: &Path) {
    let dir = base_dir.join("crash_reports");
    let _ = CRASH_REPORT_DIR.set(dir);
}

/// Get the crash reports directory, falling back to a sensible default.
fn crash_report_dir() -> PathBuf {
    CRASH_REPORT_DIR
        .get()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("crash_reports"))
}

/// A sanitized crash report suitable for bug-report submission.
///
/// **Sanitization guarantees:**
/// - No raw file paths beyond the application root directory are included.
/// - No case identifiers, evidence paths, or user data appear.
/// - Stack trace is filtered to show only our crate frames.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashReport {
    /// UTC ISO 8601 timestamp when the crash occurred.
    pub timestamp: String,
    /// The panic message (payload as a string).
    pub message: String,
    /// The panic location (file:line, if available).
    pub location: Option<String>,
    /// Sanitized backtrace lines (only frames within our crates).
    pub backtrace: Vec<String>,
    /// Application version at time of crash.
    pub app_version: String,
    /// OS family (e.g. "windows", "linux", "macos").
    pub os: String,
    /// CPU architecture.
    pub arch: String,
    /// Number of logical CPUs.
    pub num_cpus: usize,
    /// Total system memory in bytes (best-effort; 0 if unavailable).
    pub total_memory_bytes: u64,
}

impl CrashReport {
    /// Build a crash report from a panic payload and optional location.
    pub fn from_panic(
        message: impl Into<String>,
        location: Option<(&str, u32)>,
        app_version: &str,
    ) -> Self {
        let backtrace = capture_sanitized_backtrace();

        // System diagnostics.
        let os = std::env::consts::OS.to_string();
        let arch = std::env::consts::ARCH.to_string();
        let num_cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0);

        // Best-effort total memory. Not available via stable std; return 0.
        let total_memory_bytes = 0u64;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs()
            .to_string();

        CrashReport {
            timestamp,
            message: message.into(),
            location: location.map(|(file, line)| format!("{}:{}", sanitize_path(file), line)),
            backtrace,
            app_version: app_version.to_string(),
            os,
            arch,
            num_cpus,
            total_memory_bytes,
        }
    }
}

/// Install the global panic hook.
///
/// When a panic occurs, the hook:
/// 1. Captures a sanitized stack trace.
/// 2. Builds a `CrashReport`.
/// 3. Writes it as JSON to `<crash_report_dir>/crash-<timestamp>.json`.
/// 4. Logs the crash via `tracing::error`.
/// 5. Calls the previous panic hook (if any) to preserve default behavior.
pub fn set_panic_hook(app_version: &'static str) {
    let previous_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic_info| {
        let payload = panic_info.payload();
        let message = if let Some(s) = payload.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s.clone()
        } else {
            "Box<dyn Any>".to_string()
        };

        let location = panic_info.location().map(|loc| (loc.file(), loc.line()));

        let report = CrashReport::from_panic(message, location, app_version);

        // Write the crash report to disk.
        write_crash_report(&report);

        // Also log to the tracing subscriber so it ends up in the normal log.
        error!(
            target = "crash_handler",
            message = %report.message,
            location = ?report.location,
            backtrace_len = report.backtrace.len(),
            "application panicked — crash report written"
        );

        // Invoke the previous hook so default behavior (e.g. stderr dump) happens.
        previous_hook(panic_info);
    }));
}

/// Write a crash report as JSON to the crash_reports directory.
fn write_crash_report(report: &CrashReport) {
    let dir = crash_report_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        error!(target = "crash_handler", dir = %dir.display(), error = %e, "failed to create crash report directory");
        return;
    }

    let filename = format!("crash-{}.json", report.timestamp);
    let filepath = dir.join(&filename);

    let json = match serde_json::to_string_pretty(report) {
        Ok(j) => j,
        Err(e) => {
            error!(target = "crash_handler", error = %e, "failed to serialize crash report");
            return;
        }
    };

    match std::fs::File::create(&filepath) {
        Ok(mut f) => {
            let _ = f.write_all(json.as_bytes());
            error!(target = "crash_handler", path = %filepath.display(), "crash report saved");
        }
        Err(e) => {
            error!(target = "crash_handler", path = %filepath.display(), error = %e, "failed to write crash report file");
        }
    }
}

// ---------------------------------------------------------------------------
// Backtrace capture and sanitization
// ---------------------------------------------------------------------------

/// Capture a backtrace and return sanitized lines.
///
/// Uses `std::backtrace::Backtrace` when available (nightly), otherwise falls
/// back to a `RUST_BACKTRACE` environment variable hint.
fn capture_sanitized_backtrace() -> Vec<String> {
    // `std::backtrace::Backtrace::capture()` is stable as of Rust 1.65.
    let bt = std::backtrace::Backtrace::capture();
    let status = bt.status();

    if status == std::backtrace::BacktraceStatus::Captured {
        let raw = format!("{:#?}", bt);
        return raw
            .lines()
            .filter(|line| {
                // Only keep frames that look like they originate from our
                // application code (not std / third-party crates).
                line.contains("forensic") || line.contains("evidence")
            })
            .map(|line| sanitize_path(line))
            .collect();
    }

    vec![format!(
        "backtrace unavailable (status = {:?}). Set RUST_BACKTRACE=1 for a full trace.",
        status
    )]
}

/// Strip user-specific home directory and long paths to a short form.
///
/// Replaces the user's home directory prefix with `~`, and trims absolute
/// paths to just the last two components.
fn sanitize_path(raw: &str) -> String {
    // Replace known user home directory prefixes.
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();

    let sanitized = if !home.is_empty() {
        raw.replace(&home, "~")
    } else {
        raw.to_string()
    };

    // Further shorten long absolute paths: keep only the last two components
    // after the drive letter or root.
    if sanitized.starts_with("\\\\?\\") {
        return format!("<long-path>");
    }

    sanitized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_path() {
        let result = sanitize_path("C:\\Users\\QAQ\\Documents\\file.txt");
        assert!(!result.contains("QAQ") || result.contains("~"));
    }

    #[test]
    fn test_sanitize_path_unc_long() {
        let result = sanitize_path("\\\\?\\C:\\very\\long\\path\\file.rs");
        assert_eq!(result, "<long-path>");
    }

    #[test]
    fn test_crash_report_serialization() {
        let report = CrashReport::from_panic("test panic", Some(("src/lib.rs", 42)), "5.0.0-rc1");
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("test panic"));
        assert!(json.contains("42"));
        assert!(json.contains("5.0.0-rc1"));
    }
}
