use super::*;
use std::sync::Mutex;

// Serialize tests that mutate process environment variables.
static ENV_LOCK: Mutex<()> = Mutex::new(());

// -----------------------------------------------------------------------
// Original tests (kept)
// -----------------------------------------------------------------------

#[test]
fn test_sanitize_path() {
    let _guard = ENV_LOCK.lock().unwrap();
    let saved = std::env::var("USERPROFILE").ok();
    std::env::set_var("USERPROFILE", "C:\\Users\\QAQ");

    let result = sanitize_path("C:\\Users\\QAQ\\Documents\\file.txt");
    assert!(!result.contains("QAQ") || result.contains("~"));

    if let Some(v) = saved {
        std::env::set_var("USERPROFILE", v);
    } else {
        std::env::remove_var("USERPROFILE");
    }
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

// -----------------------------------------------------------------------
// New tests: path sanitization
// -----------------------------------------------------------------------

/// Path containing USERPROFILE is replaced with `~`.
#[test]
fn test_sanitize_path_removes_user_profile() {
    let _guard = ENV_LOCK.lock().unwrap();
    let saved = std::env::var("USERPROFILE").ok();
    std::env::set_var("USERPROFILE", "C:\\Users\\Investigator");

    let result = sanitize_path("C:\\Users\\Investigator\\Documents\\report.txt");
    assert!(
        !result.contains("Investigator"),
        "user name should be stripped; got `{}`",
        result
    );
    assert!(
        result.contains("~"),
        "tilde replacement expected; got `{}`",
        result
    );

    // Restore
    if let Some(v) = saved {
        std::env::set_var("USERPROFILE", v);
    } else {
        std::env::remove_var("USERPROFILE");
    }
}

/// Path containing a case root under the user profile is sanitized so no
/// user-identifiable directory appears verbatim.
#[test]
fn test_sanitize_path_removes_case_path() {
    let _guard = ENV_LOCK.lock().unwrap();
    let saved = std::env::var("USERPROFILE").ok();
    std::env::set_var("USERPROFILE", "C:\\Users\\QAQ");

    let result = sanitize_path("C:\\Users\\QAQ\\Cases\\case-001\\evidence\\ntfs.dd");
    assert!(
        !result.contains("QAQ"),
        "user directory should be stripped from case path; got `{}`",
        result
    );
    assert!(
        result.contains("~"),
        "tilde should replace user home in case path; got `{}`",
        result
    );

    if let Some(v) = saved {
        std::env::set_var("USERPROFILE", v);
    } else {
        std::env::remove_var("USERPROFILE");
    }
}

/// A path within the application install directory (no user home prefix)
/// is left intact — no spurious tilde replacement.
#[test]
fn test_sanitize_path_preserves_app_path() {
    // The real USERPROFILE won't contain "Program Files", so sanitization
    // should be a no-op for an app install path.
    let result = sanitize_path("C:\\Program Files\\ForensicsWorkbench\\app.exe");
    assert!(
        !result.contains('~'),
        "app path should not get a tilde; got `{}`",
        result
    );
    assert!(
        result.contains("ForensicsWorkbench"),
        "app directory should be preserved; got `{}`",
        result
    );
}

// -----------------------------------------------------------------------
// New tests: crash report content
// -----------------------------------------------------------------------

/// A generated crash report always carries a backtrace (at minimum a
/// fallback message when the runtime cannot capture frames).
#[test]
fn test_crash_report_contains_stack_trace() {
    let report = CrashReport::from_panic("test", Some(("src/main.rs", 99)), "5.0.0-rc1");
    assert!(
        !report.backtrace.is_empty(),
        "backtrace vec should never be empty"
    );
}

/// The serialised report must not contain raw user-profile or case paths;
/// the location file path is run through `sanitize_path`.
#[test]
fn test_crash_report_excludes_case_data() {
    let _guard = ENV_LOCK.lock().unwrap();
    let saved = std::env::var("USERPROFILE").ok();
    std::env::set_var("USERPROFILE", "C:\\Users\\QAQ");

    let report = CrashReport::from_panic(
        "safe generic panic message",
        Some(("C:\\Users\\QAQ\\Cases\\case-001\\import.rs", 10)),
        "5.0.0-rc1",
    );

    let json = serde_json::to_string(&report).unwrap();
    assert!(
        !json.contains("C:\\Users\\QAQ"),
        "raw user path must not appear in JSON"
    );
    assert!(
        json.contains('~'),
        "sanitized tilde should be present in JSON"
    );

    if let Some(v) = saved {
        std::env::set_var("USERPROFILE", v);
    } else {
        std::env::remove_var("USERPROFILE");
    }
}

// -----------------------------------------------------------------------
// New tests: file I/O
// -----------------------------------------------------------------------

/// `write_crash_report` creates a JSON file on disk inside the crash
/// reports directory.
#[test]
fn test_write_crash_report_creates_file() {
    let report = CrashReport {
        timestamp: "9999999999".to_string(),
        message: "test write to disk".to_string(),
        location: Some("test_lib.rs:1".to_string()),
        backtrace: vec!["frame_a".to_string(), "frame_b".to_string()],
        app_version: "test".to_string(),
        os: "windows".to_string(),
        arch: "x86_64".to_string(),
        num_cpus: 8,
        total_memory_bytes: 0,
    };

    write_crash_report(&report);

    let dir = crash_report_dir();
    let filepath = dir.join("crash-9999999999.json");
    assert!(
        filepath.exists(),
        "crash report file should exist at {}",
        filepath.display()
    );

    // Verify the content is valid JSON matching the report.
    let raw = std::fs::read_to_string(&filepath).unwrap();
    let parsed: CrashReport = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed.timestamp, "9999999999");
    assert_eq!(parsed.message, "test write to disk");

    // Clean up.
    let _ = std::fs::remove_file(&filepath);
    let _ = std::fs::remove_dir(&dir);
}

/// Sequential crashes produce unique filenames keyed by their timestamps.
#[test]
fn test_multiple_crash_reports_unique() {
    let report1 = CrashReport {
        timestamp: "1111111111".to_string(),
        message: "first crash".to_string(),
        location: None,
        backtrace: vec![],
        app_version: "test".to_string(),
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        num_cpus: 1,
        total_memory_bytes: 0,
    };

    let report2 = CrashReport {
        timestamp: "2222222222".to_string(),
        message: "second crash".to_string(),
        location: None,
        backtrace: vec![],
        app_version: "test".to_string(),
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        num_cpus: 1,
        total_memory_bytes: 0,
    };

    write_crash_report(&report1);
    write_crash_report(&report2);

    let dir = crash_report_dir();
    let file1 = dir.join("crash-1111111111.json");
    let file2 = dir.join("crash-2222222222.json");

    assert!(file1.exists(), "first report file must exist");
    assert!(file2.exists(), "second report file must exist");
    assert_ne!(file1, file2, "filenames must be unique per timestamp");

    // Clean up.
    let _ = std::fs::remove_file(&file1);
    let _ = std::fs::remove_file(&file2);
    let _ = std::fs::remove_dir(&dir);
}

/// An empty crash report (blank fields, no backtrace) still serialises
/// without panicking and produces valid JSON.
#[test]
fn test_empty_crash_report_handled() {
    let report = CrashReport {
        timestamp: String::new(),
        message: String::new(),
        location: None,
        backtrace: vec![],
        app_version: String::new(),
        os: String::new(),
        arch: String::new(),
        num_cpus: 0,
        total_memory_bytes: 0,
    };

    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.is_empty(), "empty report should still produce JSON");

    // Must be valid JSON.
    let _parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
}

/// The serialised crash report is valid JSON whose top-level object
/// contains every expected key.
#[test]
fn test_crash_report_json_structure() {
    let report = CrashReport::from_panic("test", Some(("src/lib.rs", 42)), "6.0.0-beta");
    let json = serde_json::to_string(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("report must be valid JSON");
    let obj = parsed
        .as_object()
        .expect("top-level JSON value must be an object");

    assert!(obj.contains_key("timestamp"), "missing `timestamp` key");
    assert!(obj.contains_key("message"), "missing `message` key");
    assert!(obj.contains_key("location"), "missing `location` key");
    assert!(obj.contains_key("backtrace"), "missing `backtrace` key");
    assert!(obj.contains_key("app_version"), "missing `app_version` key");
    assert!(obj.contains_key("os"), "missing `os` key");
    assert!(obj.contains_key("arch"), "missing `arch` key");
    assert!(obj.contains_key("num_cpus"), "missing `num_cpus` key");
    assert!(
        obj.contains_key("total_memory_bytes"),
        "missing `total_memory_bytes` key"
    );
}
