//! Unit tests for the WeChat key-recovery service: dump admission, keys
//! file placement/format, response parsing (no key leakage into the DTO),
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
fn recovery_outcome_parse_and_dto_never_carry_keys() {
    let response = serde_json::json!({
        "keys": { "message_0.db": "ab".repeat(32) },
        "matched": ["message_0.db"],
        "unmatched": ["contact.db", 42],
        "candidatesSeen": 7
    });
    let outcome = RecoveryOutcome::parse(response).expect("parse");
    assert_eq!(outcome.candidates_seen, 7);
    assert_eq!(outcome.recovered_count(), 1);
    let dto = outcome.into_dto();
    assert_eq!(dto.matched_db_names, vec!["message_0.db".to_string()]);
    // Non-string entries are dropped.
    assert_eq!(dto.unmatched_db_names, vec!["contact.db".to_string()]);
    let json = serde_json::to_value(&dto).expect("serialize");
    assert!(
        json.get("keys").is_none(),
        "DTO must not carry key material"
    );
}

#[test]
fn keys_file_round_trips_the_keyinject_format() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = keys_file_path(temp.path());
    let mut keys = Map::new();
    keys.insert("message_0.db".to_string(), Value::String("ab".repeat(32)));
    write_keys_file(&path, &keys).expect("write keys");
    let parsed: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    assert_eq!(parsed["message_0.db"], Value::String("ab".repeat(32)));
    // Temp staging file is renamed away.
    assert!(!path.with_extension("json.tmp").exists());
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
