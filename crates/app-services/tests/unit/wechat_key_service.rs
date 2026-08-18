//! Unit tests for the WeChat key-recovery service: dump admission, keys
//! file placement/format, explicit verified-key DTO projection,
//! and the analysis-run env guard.

use super::*;
use std::sync::Mutex;

/// Env-mutating tests serialize on this lock (Rust tests share the process).
static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn dump_path_must_exist() {
    let error = validate_dump_path(Path::new("Z:/path-that-must-not-exist/dump.dmp"))
        .expect_err("missing dump");
    assert!(matches!(error, PluginActionError::InvalidInput(_)));
}

#[test]
fn dump_path_must_be_a_file_with_allowed_extension() {
    let temp = tempfile::tempdir().expect("tempdir");
    assert!(
        validate_dump_path(temp.path()).is_err(),
        "directory rejected"
    );

    let bad_ext = temp.path().join("dump.bin");
    std::fs::write(&bad_ext, b"x").expect("write");
    assert!(validate_dump_path(&bad_ext).is_err(), "extension rejected");

    let dmp = temp.path().join("dump.dmp");
    std::fs::write(&dmp, b"x").expect("write");
    assert!(validate_dump_path(&dmp).is_ok());

    let raw = temp.path().join("dump.RAW");
    std::fs::write(&raw, b"x").expect("write");
    assert!(
        validate_dump_path(&raw).is_ok(),
        "extension is case-insensitive"
    );
}

#[test]
fn keys_file_lives_under_case_derived_area() {
    let path = keys_file_path(Path::new("case-root"));
    assert_eq!(
        path,
        Path::new("case-root")
            .join("derived")
            .join("wechat-keys")
            .join("keys.json")
    );
}

#[test]
fn recovery_outcome_projects_verified_keys_for_investigator_display() {
    let response = serde_json::json!({
        "keys": { "message_0.db": "ab".repeat(32) },
        "matched": ["message_0.db"],
        "unmatched": ["contact.db", 42],
        "candidatesSeen": 7
    });
    let mut outcome = RecoveryOutcome::parse(response).expect("parse");
    assert_eq!(outcome.candidates_seen, 7);
    assert_eq!(outcome.recovered_count(), 1);
    outcome.apply_display_names(&BTreeMap::from([(
        "message_0.db".to_string(),
        "[P2]/wechat/message_0.db".to_string(),
    )]));
    let dto = outcome.into_dto();
    assert_eq!(
        dto.matched_db_names,
        vec!["[P2]/wechat/message_0.db".to_string()]
    );
    // Non-string entries are dropped.
    assert_eq!(dto.unmatched_db_names, vec!["contact.db".to_string()]);
    let json = serde_json::to_value(&dto).expect("serialize");
    assert_eq!(json["recoveredKeys"][0]["keyHex"], "ab".repeat(32));
}

#[test]
fn keys_file_round_trips_the_keyinject_format() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = keys_file_path(temp.path());
    let mut keys = Map::new();
    keys.insert("message_0.db".to_string(), Value::String("ab".repeat(32)));
    keys.insert("existing.db".to_string(), Value::String("ef".repeat(32)));
    write_keys_file(&path, &keys).expect("write keys");
    let parsed: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    assert_eq!(parsed["message_0.db"], Value::String("ab".repeat(32)));
    let replacement =
        Map::from_iter([("message_0.db".to_string(), Value::String("cd".repeat(32)))]);
    write_keys_file(&path, &replacement).expect("replace keys");
    let replaced: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read replacement"))
            .expect("replacement json");
    assert_eq!(replaced["message_0.db"], Value::String("cd".repeat(32)));
    assert_eq!(replaced["existing.db"], Value::String("ef".repeat(32)));
    assert!(std::fs::read_dir(path.parent().expect("parent"))
        .expect("read key dir")
        .all(|entry| !entry
            .expect("entry")
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
}

#[test]
fn recovery_outcome_display_names_do_not_change_secret_key_ids() {
    let response = serde_json::json!({
        "keys": { "file-entry-1": "ab".repeat(32) },
        "matched": ["file-entry-1"],
        "unmatched": ["file-entry-2"],
        "candidatesSeen": 1
    });
    let mut outcome = RecoveryOutcome::parse(response).expect("parse");
    outcome.apply_display_names(&BTreeMap::from([
        (
            "file-entry-1".to_string(),
            "[P2]/wechat/message_0.db".to_string(),
        ),
        (
            "file-entry-2".to_string(),
            "[P2]/wechat/contact.db".to_string(),
        ),
    ]));
    assert!(outcome.keys.contains_key("file-entry-1"));
    let dto = outcome.into_dto();
    assert_eq!(dto.matched_db_names, vec!["[P2]/wechat/message_0.db"]);
    assert_eq!(dto.unmatched_db_names, vec!["[P2]/wechat/contact.db"]);
    assert_eq!(
        dto.recovered_keys[0].database_name,
        "[P2]/wechat/message_0.db"
    );
    assert_eq!(dto.recovered_keys[0].key_hex, "ab".repeat(32));
}

#[test]
fn recovery_outcome_drops_unknown_ids_and_malformed_keys() {
    let response = serde_json::json!({
        "keys": {
            "known-good": "ab".repeat(32),
            "known-bad": "not-a-key",
            "not-requested": "cd".repeat(32)
        },
        "matched": ["known-good", "known-bad", "not-requested"],
        "unmatched": ["known-missing", "not-requested"],
        "candidatesSeen": 3
    });
    let requested = BTreeMap::from([
        ("known-good".to_string(), "good.db".to_string()),
        ("known-bad".to_string(), "bad.db".to_string()),
        ("known-missing".to_string(), "missing.db".to_string()),
    ]);
    let mut outcome = RecoveryOutcome::parse(response).expect("parse");
    outcome.retain_valid_keys(&requested);
    assert_eq!(outcome.keys.len(), 1);
    assert!(outcome.keys.contains_key("known-good"));
    assert_eq!(outcome.matched_db_names, vec!["known-good"]);
    assert_eq!(outcome.unmatched_db_names, vec!["known-missing"]);
}

#[test]
fn env_guard_activates_only_with_keys_file_and_restores() {
    let _serial = ENV_LOCK.lock().expect("env lock");
    std::env::remove_var(WECHAT_KEYS_ENV);
    let temp = tempfile::tempdir().expect("tempdir");

    // No keys file: guard inactive, env untouched.
    assert!(WeChatKeysEnvGuard::activate(temp.path()).is_none());
    assert!(std::env::var_os(WECHAT_KEYS_ENV).is_none());

    let mut keys = Map::new();
    keys.insert("a.db".to_string(), Value::String("cd".repeat(32)));
    write_keys_file(&keys_file_path(temp.path()), &keys).expect("write keys");
    {
        let guard = WeChatKeysEnvGuard::activate(temp.path()).expect("active guard");
        assert_eq!(
            std::env::var_os(WECHAT_KEYS_ENV).expect("env set"),
            keys_file_path(temp.path()).into_os_string()
        );
        drop(guard);
    }
    assert!(std::env::var_os(WECHAT_KEYS_ENV).is_none(), "env restored");
}
