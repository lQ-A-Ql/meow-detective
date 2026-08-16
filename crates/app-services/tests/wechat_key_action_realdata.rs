//! Real-sample end-to-end validation of the WeChat key-recovery action
//! channel: the real plugin DLL is driven through `meow_plugin_action`
//! against a real memory dump and the real encrypted databases extracted
//! from the private Windows image (same fixture family as
//! `wechat_realdata_validation`).
//!
//! Run:
//!   $env:FORENSICS_WECHAT_DUMP='<memory dump (.dmp) path>'
//!   $env:FORENSICS_WECHAT_EXTRACT_DIR='target\tmp\wechat-extract'
//!   cargo test -p app-services --test wechat_key_action_realdata -- --ignored --nocapture
#![cfg(windows)]

mod wechat_plugin_util;

use app_services::plugin_loader::load_plugins_from_dirs;
use base64::Engine as _;

/// Database files produced by `examples/extract_wechat.rs` (file name ==
/// dbName key used by the key-injection format).
const DB_FILES: &[&str] = &[
    "contact.db",
    "session.db",
    "sns.db",
    "favorite.db",
    "message_0.db",
    "biz_message_0.db",
    "message_fts.db",
    "message_resource.db",
];

fn required_env(name: &str) -> std::path::PathBuf {
    std::env::var_os(name)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| panic!("set {name}"))
}

#[test]
#[ignore = "requires the private memory dump and extracted WeChat files"]
fn recover_keys_action_against_real_dump() {
    let dir = wechat_plugin_util::stage_wechat_plugin();
    let plugins = load_plugins_from_dirs(&[dir.path().to_path_buf()]);
    assert_eq!(plugins.len(), 1);
    let plugin = plugins.into_iter().next().expect("one plugin");

    // Action channel handshake: the WeChat plugin must export it and
    // describe exactly one file-input action.
    assert!(plugin.has_actions());
    let actions = plugin.describe_actions().expect("describe");
    assert_eq!(actions[0]["id"], "recoverKeys");
    assert_eq!(actions[0]["inputKind"], "file");

    let dump = required_env("FORENSICS_WECHAT_DUMP");
    let extract = required_env("FORENSICS_WECHAT_EXTRACT_DIR");
    let mut db_pages = serde_json::Map::new();
    for file in DB_FILES {
        let data = std::fs::read(extract.join(file))
            .unwrap_or_else(|error| panic!("read {file}: {error}"));
        assert!(data.len() >= 4096, "{file} shorter than one page");
        db_pages.insert(
            file.to_string(),
            serde_json::Value::String(
                base64::engine::general_purpose::STANDARD.encode(&data[..4096]),
            ),
        );
    }
    let response = plugin
        .call_action(
            "recoverKeys",
            &serde_json::json!({
                "dumpPath": dump.to_string_lossy(),
                "dbPages": serde_json::Value::Object(db_pages),
            }),
        )
        .expect("recoverKeys action");

    let candidates_seen = response["candidatesSeen"].as_u64().expect("candidatesSeen");
    let keys = response["keys"].as_object().expect("keys");
    let matched = response["matched"].as_array().expect("matched");
    let unmatched = response["unmatched"].as_array().expect("unmatched");
    println!(
        "candidatesSeen={candidates_seen} recovered={} matched={matched:?} unmatched={unmatched:?}",
        keys.len()
    );
    assert!(candidates_seen > 0, "dump must yield key candidates");
    assert!(!keys.is_empty(), "at least one database key must validate");
    // Every matched database has a key entry; unmatched ones do not. Never
    // print or assert on key material itself.
    for name in matched {
        let name = name.as_str().expect("matched name");
        assert!(keys.contains_key(name), "matched {name} without key");
        let hex = keys[name].as_str().expect("hex key");
        assert_eq!(hex.len(), 64, "key must be 32 bytes hex");
        assert!(hex.bytes().all(|b| b.is_ascii_hexdigit()));
    }
    for name in unmatched {
        assert!(!keys.contains_key(name.as_str().expect("unmatched name")));
    }
}
