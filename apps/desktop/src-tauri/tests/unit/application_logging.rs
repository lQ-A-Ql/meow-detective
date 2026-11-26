use super::install_panic_hook;
use infrastructure::logging::{LogConfig, LogRuntime};
use tracing_subscriber::prelude::*;

#[test]
fn panic_hook_flushes_error_before_failed_child_exits() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "application_logging::tests::panic_hook_child",
            "--ignored",
        ])
        .env("FORENSICS_LOG_PANIC_OUTPUT", directory.path());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().unwrap();
    assert!(!output.status.success());
    let session = std::fs::read_dir(directory.path())
        .unwrap()
        .map(Result::unwrap)
        .find(|entry| entry.path().is_dir())
        .unwrap();
    let text = std::fs::read_to_string(session.path().join("runtime_rCURRENT.jsonl")).unwrap();
    let records: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["level"], "error");
    assert_eq!(records[0]["message"], "Application thread panicked");
    assert!(!text.contains("private-value"));
    assert!(text.contains("password=<redacted>"));
}

#[test]
#[ignore = "child process used by panic_hook_flushes_error_before_failed_child_exits"]
fn panic_hook_child() {
    let directory = std::env::var_os("FORENSICS_LOG_PANIC_OUTPUT").unwrap();
    let (runtime, _guard) = LogRuntime::start(LogConfig::new(directory.into())).unwrap();
    tracing_subscriber::registry()
        .with(runtime.layer())
        .try_init()
        .unwrap();
    install_panic_hook(runtime);
    panic!("logging validation password=private-value");
}
