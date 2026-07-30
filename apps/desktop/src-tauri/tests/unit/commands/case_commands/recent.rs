use super::super::{
    lifecycle_support::meta_to_dto,
    recent::{read_recent_cases, recent_cases_path, remember_recent_case, save_recent_cases},
};
use app_services::case_service;
use std::sync::{Mutex, OnceLock};
use transport::dto::RecentCaseDto;
use uuid::Uuid;

fn recent_cases_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn remember_recent_case_uses_actual_case_directory() {
    let _lock = recent_cases_test_lock().lock().unwrap();
    let parent = std::env::temp_dir().join(format!(
        "Meow_Detective-recent-case-test-{}",
        Uuid::new_v4()
    ));
    let previous = std::env::var_os("FORENSICS_RECENT_CASES_DIR");
    std::env::set_var("FORENSICS_RECENT_CASES_DIR", parent.join("recent-state"));

    let active = case_service::create_case(&parent, "recent-case", Some("tester"))
        .expect("create recent case fixture");
    let dto = meta_to_dto(&active.meta);
    let actual_case_root = active.case_root.clone();
    drop(active);

    remember_recent_case(&actual_case_root, &dto).expect("remember recent case");
    let recent = read_recent_cases().expect("read recent cases");

    assert_eq!(recent[0].case_root, actual_case_root.display().to_string());
    assert_ne!(recent[0].case_root, parent.display().to_string());

    let mut remaining = recent;
    remaining.retain(|item| item.case_root != actual_case_root.display().to_string());
    save_recent_cases(&remaining).expect("restore recent cases");
    restore_recent_cases_dir(previous);
    std::fs::remove_dir_all(parent).ok();
}

#[test]
fn recent_cases_file_is_restricted_and_round_trips() {
    let _lock = recent_cases_test_lock().lock().unwrap();
    let dir = std::env::temp_dir().join(format!(
        "Meow_Detective-recent-cases-security-test-{}",
        Uuid::new_v4()
    ));
    let previous = std::env::var_os("FORENSICS_RECENT_CASES_DIR");
    std::env::set_var("FORENSICS_RECENT_CASES_DIR", &dir);

    let active =
        case_service::create_case(&dir, "Secure Case", Some("tester")).expect("create case");
    let summary = meta_to_dto(&active.meta);
    let cases = vec![RecentCaseDto {
        case_root: active.case_root.display().to_string(),
        name: summary.name,
        opened_at: chrono::Utc::now().to_rfc3339(),
    }];

    let saved = save_recent_cases(&cases);
    assert!(saved.is_ok(), "save_recent_cases failed: {saved:?}");

    let path = recent_cases_path().expect("resolve recent cases path");
    assert!(path.exists(), "recent cases file should exist");

    let loaded = read_recent_cases().expect("read recent cases");
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "Secure Case");

    restore_recent_cases_dir(previous);
    std::fs::remove_dir_all(&dir).ok();
}

fn restore_recent_cases_dir(previous: Option<std::ffi::OsString>) {
    match previous {
        Some(value) => std::env::set_var("FORENSICS_RECENT_CASES_DIR", value),
        None => std::env::remove_var("FORENSICS_RECENT_CASES_DIR"),
    }
}
