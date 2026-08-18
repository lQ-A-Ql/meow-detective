//! WeChat plugin regression (`plugins-src/wechat`): synthetic plaintext
//! SQLite databases, encrypted stand-ins, and the plaintext side files are
//! driven across the real DLL boundary and the resulting artifacts are
//! asserted field by field — family whitelist acceptance, the WCDB
//! encryption warning, path self-filtering, and host-enforced provenance.
#![cfg(windows)]

mod wechat_plugin_util;

use app_services::plugin_loader::{load_plugins_from_dirs, PluginExtractor};
use artifacts_core::{ArtifactCompanion, ArtifactContext, ArtifactExtractor, VecSink};
use domain::FileEntryId;
use rusqlite::Connection;

const FILE_ID: &str = "ds:1:wechat-1";
const DATA_PREFIX: &str =
    "[P2]/Users/admin/Documents/xwechat_files/wxid_zuaa9igqlro22_eef8/db_storage";

/// Build a synthetic plaintext database: run `schema`, serialize to bytes.
fn synthetic_db(schema: &str) -> Vec<u8> {
    let conn = Connection::open_in_memory().expect("in-memory db");
    conn.execute_batch(schema).expect("fixture schema");
    let data = conn
        .serialize(rusqlite::DatabaseName::Main)
        .expect("serialize fixture");
    data.to_vec()
}

fn load_wechat_plugin(dir: &std::path::Path) -> PluginExtractor {
    let plugins = load_plugins_from_dirs(&[dir.to_path_buf()]);
    assert_eq!(plugins.len(), 1, "exactly one plugin DLL staged");
    let plugin = plugins.into_iter().next().expect("one plugin");
    assert_eq!(plugin.id(), "meow.plugin.wechat");
    // Host-side path matching: suffix for *.db, exact file name otherwise.
    assert!(plugin.supports_path(&format!("{DATA_PREFIX}/message/message_0.db")));
    assert!(plugin.supports_path("[P2]/Program Files/Tencent/Weixin/4.1.8.67/plugin_info.ini"));
    assert!(plugin.supports_path("[P0]/anywhere/cloud_account.txt"));
    assert!(!plugin.supports_path("[P0]/anywhere/other.txt"));
    plugin
}

fn run_plugin(
    plugin: &PluginExtractor,
    file_path: &str,
    data: Vec<u8>,
) -> (VecSink, Result<artifacts_core::ExtractorReport, String>) {
    let ctx = ArtifactContext {
        file_id: FileEntryId(FILE_ID.to_string()),
        file_path: file_path.to_string(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = plugin.run(ctx, &mut sink);
    (sink, report)
}

#[test]
fn plaintext_database_is_deep_parsed_and_provenanced() {
    let dir = wechat_plugin_util::stage_wechat_plugin();
    let plugin = load_wechat_plugin(dir.path());
    let data = synthetic_db(
        "CREATE TABLE message (id INTEGER PRIMARY KEY, content TEXT);
        INSERT INTO message (content) VALUES ('hello'), ('world');
        CREATE TABLE session (id INTEGER PRIMARY KEY, name TEXT);
        INSERT INTO session (name) VALUES ('filehelper');",
    );
    let path = format!("{DATA_PREFIX}/message/message_0.db");
    let (sink, report) = run_plugin(&plugin, &path, data);
    let report = report.expect("plugin run must succeed");
    assert_eq!(report.artifacts_found, 1);
    // No family-whitelist rejections, no warnings on the plaintext path.
    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.contains("undeclared family")),
        "unexpected plugin warning: {:?}",
        report.errors
    );

    let artifact = &sink.artifacts[0];
    assert_eq!(artifact.family, "WeChatDatabase");
    assert_eq!(artifact.title, "message/message_0.db");
    assert_eq!(artifact.attrs["wxid"], "wxid_zuaa9igqlro22_eef8");
    assert_eq!(artifact.attrs["category"], "message");
    assert_eq!(artifact.attrs["dbName"], "message_0.db");
    assert_eq!(artifact.attrs["encrypted"], false);
    assert_eq!(artifact.attrs["tableCount"], 2);
    assert_eq!(
        artifact.attrs["tableList"],
        serde_json::json!(["message", "session"])
    );
    assert_eq!(artifact.attrs["rowCounts"]["message"], 2);
    assert_eq!(artifact.attrs["rowCounts"]["session"], 1);
    assert_eq!(report.timeline_events, 0);
    // Host-enforced provenance.
    assert_eq!(
        artifact.source_object_id.as_ref().map(|id| id.0.as_str()),
        Some(FILE_ID)
    );
    assert_eq!(artifact.extractor_id.as_deref(), Some("meow.plugin.wechat"));
    assert_eq!(artifact.extractor_version.as_deref(), Some("0.3.0"));
    assert_eq!(artifact.source_attribution.as_deref(), Some(path.as_str()));
}

#[test]
fn host_passes_database_wal_companion_across_abi() {
    let dir = wechat_plugin_util::stage_wechat_plugin();
    let plugin = load_wechat_plugin(dir.path());
    let data = synthetic_db("CREATE TABLE message (id INTEGER PRIMARY KEY);");
    let path = format!("{DATA_PREFIX}/message/message_0.db");
    let context = ArtifactContext {
        file_id: FileEntryId(FILE_ID.to_string()),
        file_path: path.clone(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let companions = [ArtifactCompanion {
        file_id: FileEntryId("ds:1:wechat-wal-1".to_string()),
        file_path: format!("{path}-wal"),
        data: Vec::new(),
    }];
    let mut sink = VecSink::new();

    plugin
        .run_with_companions(context, &companions, &mut sink)
        .expect("plugin run with WAL companion");

    assert_eq!(sink.artifacts[0].attrs["walPresent"], true);
}

#[test]
fn encrypted_database_is_inventory_with_warning() {
    let dir = wechat_plugin_util::stage_wechat_plugin();
    let plugin = load_wechat_plugin(dir.path());
    let data: Vec<u8> = (0..1024u32)
        .map(|i| (i.wrapping_mul(2654435761) % 251) as u8)
        .collect();
    let path = format!("{DATA_PREFIX}/contact/contact.db");
    let (sink, report) = run_plugin(&plugin, &path, data);
    let report = report.expect("encrypted inventory is not an error");
    assert_eq!(report.artifacts_found, 1);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("WCDB/SQLCipher") && e.contains("无法离线恢复")),
        "encryption warning must surface: {:?}",
        report.errors
    );
    let artifact = &sink.artifacts[0];
    assert_eq!(artifact.family, "WeChatDatabase");
    assert_eq!(artifact.attrs["encrypted"], true);
    assert_eq!(artifact.attrs["category"], "contact");
    assert!(!artifact.attrs.contains_key("tableList"));
}

#[test]
fn install_and_account_side_files_are_parsed() {
    let dir = wechat_plugin_util::stage_wechat_plugin();
    let plugin = load_wechat_plugin(dir.path());

    let ini = b"[plugin]\r\nWeChatPlayer=1.0.0.12\r\n".to_vec();
    let (sink, report) = run_plugin(
        &plugin,
        "[P2]/Program Files/Tencent/Weixin/4.1.8.67/plugin_info.ini",
        ini,
    );
    let report = report.expect("plugin_info.ini parses");
    assert_eq!(report.artifacts_found, 1);
    let artifact = &sink.artifacts[0];
    assert_eq!(artifact.family, "WeChatInstall");
    assert_eq!(artifact.attrs["installVersion"], "4.1.8.67");
    assert_eq!(
        artifact.attrs["pluginVersions"]["plugin.WeChatPlayer"],
        "1.0.0.12"
    );

    let (sink, report) = run_plugin(
        &plugin,
        "[P2]/Users/admin/AppData/Roaming/Tencent/xwechat/ilink/wechat/cloud_account.txt",
        b"kTdiKeyCloudSession=\r\n".to_vec(),
    );
    assert_eq!(report.expect("cloud_account parses").artifacts_found, 1);
    assert_eq!(sink.artifacts[0].family, "WeChatAccount");
    assert_eq!(sink.artifacts[0].attrs["hasCloudSession"], false);

    let blob: Vec<u8> = (0..180u32).map(|i| (i % 256) as u8).collect();
    let (sink, report) = run_plugin(
        &plugin,
        "[P2]/Users/admin/AppData/Roaming/Tencent/xwechat/login/wxid_zuaa9igqlro22/key_info.dat",
        blob,
    );
    assert_eq!(report.expect("key_info.dat parses").artifacts_found, 1);
    assert_eq!(sink.artifacts[0].attrs["wxid"], "wxid_zuaa9igqlro22");
    assert_eq!(sink.artifacts[0].attrs["keyInfoPresent"], true);
    assert_eq!(sink.artifacts[0].attrs["sizeBytes"], 180);

    let (sink, report) = run_plugin(
        &plugin,
        "[P2]/Users/admin/AppData/Roaming/Tencent/xwechat/ilink/kvcomm/config.ini",
        b"kv_clientversion=41080067\r\n".to_vec(),
    );
    assert_eq!(report.expect("config.ini parses").artifacts_found, 1);
    assert_eq!(sink.artifacts[0].family, "WeChatInstall");
    assert_eq!(
        sink.artifacts[0].attrs["settings"]["kv_clientversion"],
        "41080067"
    );
}

#[test]
fn non_wechat_paths_are_silent_ok() {
    let dir = wechat_plugin_util::stage_wechat_plugin();
    let plugin = load_wechat_plugin(dir.path());

    // A `.db` outside the WeChat data directory matches "*.db" in the host
    // path filter but is rejected by the plugin's own self-filter.
    let (sink, report) = run_plugin(
        &plugin,
        "[P0]/Windows/System32/config/SAM.db",
        b"garbage".to_vec(),
    );
    let report = report.expect("out-of-scope path is not an error");
    assert_eq!(report.artifacts_found, 0);
    assert!(sink.artifacts.is_empty());

    // A `.db` under xwechat_files but outside db_storage is likewise out.
    let (sink, report) = run_plugin(
        &plugin,
        "[P2]/Users/admin/Documents/xwechat_files/wxid_x/backup/note.db",
        b"garbage".to_vec(),
    );
    let report = report.expect("non-db_storage db is not an error");
    assert_eq!(report.artifacts_found, 0);
    assert!(sink.artifacts.is_empty());

    // Same-named side files outside the Tencent/xwechat markers.
    let (sink, report) = run_plugin(
        &plugin,
        "[P0]/opt/app/config.ini",
        b"kv_clientversion=1".to_vec(),
    );
    assert_eq!(report.expect("foreign config.ini").artifacts_found, 0);
    assert!(sink.artifacts.is_empty());
}

#[test]
fn corrupt_plaintext_db_fails_closed_then_plugin_recovers() {
    let dir = wechat_plugin_util::stage_wechat_plugin();
    let plugin = load_wechat_plugin(dir.path());

    let mut corrupt = b"SQLite format 3\0".to_vec();
    corrupt.extend_from_slice(&[0xFFu8; 256]);
    let path = format!("{DATA_PREFIX}/sns/sns.db");
    let (sink, report) = run_plugin(&plugin, &path, corrupt);
    let error = match report {
        Ok(_) => panic!("corrupt database must surface a typed error"),
        Err(error) => error,
    };
    assert!(
        error.contains("ParseError"),
        "expected ParseError, got: {error}"
    );
    assert!(sink.artifacts.is_empty());

    // No abort, no poisoning: the same plugin still parses a valid file.
    let data = synthetic_db("CREATE TABLE feeds (id INTEGER PRIMARY KEY);");
    let (sink, report) = run_plugin(&plugin, &path, data);
    assert_eq!(report.expect("plugin still works").artifacts_found, 1);
    assert_eq!(sink.artifacts[0].attrs["category"], "sns");
}
