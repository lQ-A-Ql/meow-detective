use super::*;
use chrono::Utc;
use domain::FileEntryId;
use serde_json::Value;

fn candidate(path: &str) -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: FileEntryId(path.to_string()),
        data_source_id: "ds-1".to_string(),
        path: path.to_string(),
        size: 1024,
        evidence_kind: "registry_hive".to_string(),
        parser: "registry".to_string(),
        category: "Registry".to_string(),
    }
}

#[test]
fn sam_user_artifact_carries_subject_attribution() {
    let ts = Utc::now();
    let user = artifacts_windows::SamUser {
        username: "alice".to_string(),
        rid: 1001,
        sid: "S-1-5-21-1-2-3-1001".to_string(),
        full_name: "Alice Liddell".to_string(),
        comment: String::new(),
        home_dir: String::new(),
        profile_path: "C:\\Users\\alice".to_string(),
        last_login: Some(ts),
        password_last_set: Some(ts),
        account_disabled: false,
        account_locked: false,
        admin_count: 0,
        login_count: 42,
        group_memberships: vec!["Users".to_string()],
        password_hash: Some(
            "aad3b435b51404eeaad3b435b51404ee:31d6cfe0d16ae931b73c59d7e0c089c0".to_string(),
        ),
        password_hash_type: Some("NTLM".to_string()),
    };
    let info = artifacts_windows::SamInfo {
        users: vec![user],
        groups: Vec::new(),
        password_policy: None,
        txlog_applied: false,
        txlog_timestamps: Vec::new(),
        warnings: Vec::new(),
    };
    let outcome = sam_user_artifacts(&candidate("Windows/System32/config/SAM"), &info);
    assert_eq!(outcome.artifacts.len(), 1);
    let art = &outcome.artifacts[0];
    assert_eq!(art.family, "RegistrySamUser");
    assert_eq!(
        art.source_object_id.as_ref().map(|id| id.0.as_str()),
        Some("Windows/System32/config/SAM")
    );
    assert_eq!(
        art.attrs.get("subjectSid"),
        Some(&Value::String("S-1-5-21-1-2-3-1001".to_string()))
    );
    assert_eq!(
        art.attrs.get("subjectUsername"),
        Some(&Value::String("alice".to_string()))
    );
    assert!(outcome.timeline_events.len() >= 2);
    assert!(outcome
        .timeline_events
        .iter()
        .any(|e| e.event_type == "REGISTRY_LAST_LOGIN"));
    assert!(outcome
        .timeline_events
        .iter()
        .any(|e| e.event_type == "REGISTRY_ACCOUNT_CREATED"));
}

#[test]
fn ntuser_user_assist_derives_subject_username_and_emits_timeline() {
    let info = artifacts_windows::NtuserInfo {
        run_keys: Vec::new(),
        recent_docs: Vec::new(),
        ua_entries: vec![artifacts_windows::UserAssistEntry {
            executable_path: "C:\\Windows\\notepad.exe".to_string(),
            run_count: 3,
            last_run: Some("2026-06-01T10:00:00+00:00".to_string()),
            focus_time_ms: 1200,
            session_id: 0,
        }],
        typed_urls: Vec::new(),
        word_wheel_query: Vec::new(),
        mount_points: Vec::new(),
        open_save_mru: Vec::new(),
        last_visited_mru: Vec::new(),
        run_mru: Vec::new(),
        default_browser: None,
        txlog_applied: false,
        txlog_timestamps: Vec::new(),
        warnings: Vec::new(),
    };
    let outcome = user_assist_artifacts(&candidate("Users/alice/NTUSER.DAT"), &info);
    assert_eq!(outcome.artifacts.len(), 1);
    let art = &outcome.artifacts[0];
    assert_eq!(art.family, "RegistryUserAssist");
    assert_eq!(
        art.attrs.get("subjectUsername"),
        Some(&Value::String("alice".to_string()))
    );
    assert_eq!(outcome.timeline_events.len(), 1);
    assert_eq!(
        outcome.timeline_events[0].event_type,
        "REGISTRY_USER_ASSIST_EXEC"
    );
}

#[test]
fn system_shutdown_emits_timeline_event() {
    let entries = vec![artifacts_windows::ShutdownTimeEntry {
        key_path: "ControlSet001\\Control\\Windows\\ShutdownTime".to_string(),
        shutdown_time: "2026-06-02T18:30:00+00:00".to_string(),
    }];
    let outcome = shutdown_time_artifacts(&candidate("Windows/System32/config/SYSTEM"), &entries);
    assert_eq!(outcome.artifacts.len(), 1);
    assert_eq!(outcome.artifacts[0].family, "RegistryShutdownTime");
    assert_eq!(outcome.timeline_events.len(), 1);
    assert_eq!(outcome.timeline_events[0].event_type, "REGISTRY_SHUTDOWN");
}
