//! BT Panel plugin regression (`plugins-src/bt_panel`): synthetic panel
//! SQLite databases are driven across the real DLL boundary and the
//! resulting artifacts are asserted field by field — family whitelist
//! acceptance, credential redaction, timeline events, and host-enforced
//! provenance.
#![cfg(windows)]

mod bt_panel_plugin_util;

use app_services::plugin_loader::{load_plugins_from_dirs, PluginExtractor};
use artifacts_core::{ArtifactContext, ArtifactExtractor, VecSink};
use domain::FileEntryId;
use rusqlite::Connection;

const FILE_ID: &str = "ds:1:bt-1";
const PANEL_PREFIX: &str = "[P2]/www/server/panel/data/";

/// Build a synthetic panel database: run `schema`, serialize to bytes.
fn synthetic_db(schema: &str) -> Vec<u8> {
    let conn = Connection::open_in_memory().expect("in-memory db");
    conn.execute_batch(schema).expect("fixture schema");
    let data = conn
        .serialize(rusqlite::DatabaseName::Main)
        .expect("serialize fixture");
    data.to_vec()
}

fn load_bt_panel_plugin(dir: &std::path::Path) -> PluginExtractor {
    let plugins = load_plugins_from_dirs(&[dir.to_path_buf()]);
    assert_eq!(plugins.len(), 1, "exactly one plugin DLL staged");
    let plugin = plugins.into_iter().next().expect("one plugin");
    assert_eq!(plugin.id(), "meow.plugin.bt_panel");
    assert!(plugin.supports_path(&format!("{PANEL_PREFIX}default.db")));
    plugin
}

fn run_plugin(
    plugin: &PluginExtractor,
    basename: &str,
    data: Vec<u8>,
) -> (VecSink, Result<artifacts_core::ExtractorReport, String>) {
    let ctx = ArtifactContext {
        file_id: FileEntryId(FILE_ID.to_string()),
        file_path: format!("{PANEL_PREFIX}{basename}"),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = plugin.run(ctx, &mut sink);
    (sink, report)
}

#[test]
fn account_db_artifacts_are_redacted_and_provenanced() {
    let dir = bt_panel_plugin_util::stage_bt_panel_plugin();
    let plugin = load_bt_panel_plugin(dir.path());
    let data = synthetic_db(
        "CREATE TABLE users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT, password TEXT, salt TEXT,
            login_ip TEXT, login_time TEXT
        );
        INSERT INTO users (username, password, salt, login_ip, login_time)
        VALUES ('admin', 'b59c67bf196a4758191e42f76670ceba', 's3cr3t',
                '10.0.0.8', '2025-06-01 08:00:00');",
    );
    let (sink, report) = run_plugin(&plugin, "default.db", data);
    let report = report.expect("plugin run must succeed");
    assert_eq!(report.artifacts_found, 1);
    // No family-whitelist rejections, no parser warnings beyond the
    // (expected) UTC timezone assumption note.
    for error in &report.errors {
        assert!(error.contains("UTC"), "unexpected plugin warning: {error}");
        assert!(!error.contains("undeclared family"));
    }

    let artifact = &sink.artifacts[0];
    assert_eq!(artifact.family, "BtPanelAccount");
    assert_eq!(artifact.title, "admin");
    assert_eq!(artifact.attrs["username"], "admin");
    assert_eq!(artifact.attrs["hasPasswordHash"], true);
    assert_eq!(
        artifact.attrs["passwordAlgorithm"],
        "md5(md5(md5(password)+'_bt.cn')+salt)"
    );
    assert_eq!(artifact.attrs["loginIp"], "10.0.0.8");
    assert_eq!(artifact.attrs["loginTimeUtc"], "2025-06-01T08:00:00Z");
    // Redaction: hash and salt material must not survive the boundary.
    let dump = serde_json::to_string(&sink.artifacts).expect("dump");
    assert!(!dump.contains("b59c67bf196a4758191e42f76670ceba"));
    assert!(!dump.contains("s3cr3t"));
    // Host-enforced provenance.
    assert_eq!(
        artifact.source_object_id.as_ref().map(|id| id.0.as_str()),
        Some(FILE_ID)
    );
    assert_eq!(
        artifact.extractor_id.as_deref(),
        Some("meow.plugin.bt_panel")
    );
    assert_eq!(artifact.extractor_version.as_deref(), Some("0.1.0"));
    assert_eq!(
        artifact.source_attribution.as_deref(),
        Some(format!("{PANEL_PREFIX}default.db").as_str())
    );
}

#[test]
fn site_db_joins_domains() {
    let dir = bt_panel_plugin_util::stage_bt_panel_plugin();
    let plugin = load_bt_panel_plugin(dir.path());
    let data = synthetic_db(
        "CREATE TABLE sites (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT, path TEXT, status TEXT, ps TEXT, addtime TEXT
        );
        INSERT INTO sites (name, path, status, addtime)
        VALUES ('shop.example.com', '/www/wwwroot/shop', '1', '2025-05-20 10:00:00');
        CREATE TABLE domain (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            pid INTEGER, name TEXT, port INTEGER, addtime TEXT
        );
        INSERT INTO domain (pid, name, port)
        VALUES (1, 'shop.example.com', 80), (1, 'www.shop.example.com', 443);",
    );
    let (sink, report) = run_plugin(&plugin, "db/site.db", data);
    let report = report.expect("plugin run must succeed");
    assert_eq!(report.artifacts_found, 1);
    let artifact = &sink.artifacts[0];
    assert_eq!(artifact.family, "BtPanelSite");
    assert_eq!(artifact.attrs["path"], "/www/wwwroot/shop");
    assert_eq!(artifact.attrs["statusText"], "running");
    assert_eq!(
        artifact.attrs["domains"],
        serde_json::json!(["shop.example.com:80", "www.shop.example.com:443"])
    );
}

#[test]
fn log_db_produces_timeline_events() {
    let dir = bt_panel_plugin_util::stage_bt_panel_plugin();
    let plugin = load_bt_panel_plugin(dir.path());
    let data = synthetic_db(
        "CREATE TABLE logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT, type TEXT, log TEXT, addtime TEXT
        );
        INSERT INTO logs (type, log, addtime)
        VALUES ('用户登录', '用户[admin]登录成功', '2025-06-01 08:00:01');",
    );
    let (sink, report) = run_plugin(&plugin, "db/log.db", data);
    let report = report.expect("plugin run must succeed");
    assert_eq!(report.artifacts_found, 1);
    assert_eq!(report.timeline_events, 1);
    let event = &sink.timeline_events[0];
    assert_eq!(event.event_type, "BT_PANEL_OPERATION");
    assert_eq!(
        event.timestamp,
        chrono::DateTime::parse_from_rfc3339("2025-06-01T08:00:01Z")
            .expect("timestamp")
            .with_timezone(&chrono::Utc)
    );
    assert!(event.description.contains("admin"));
    assert_eq!(event.parser_id.as_deref(), Some("meow.plugin.bt_panel"));
}

#[test]
fn non_panel_and_unknown_db_paths_are_silent_ok() {
    let dir = bt_panel_plugin_util::stage_bt_panel_plugin();
    let plugin = load_bt_panel_plugin(dir.path());

    // A `.db` outside the panel directory matches "*.db" in the host path
    // filter but is rejected by the plugin's own directory self-filter.
    let (sink, report) = {
        let ctx = ArtifactContext {
            file_id: FileEntryId(FILE_ID.to_string()),
            file_path: "[P0]/var/lib/snapd/state.db".to_string(),
            reader: Box::new(std::io::Cursor::new(b"garbage".to_vec())),
        };
        let mut sink = VecSink::new();
        let report = plugin.run(ctx, &mut sink);
        (sink, report)
    };
    let report = report.expect("out-of-scope path is not an error");
    assert_eq!(report.artifacts_found, 0);
    assert!(sink.artifacts.is_empty());

    // Unknown DB names inside the panel directory (docker.db, panel.db,
    // task.db) are likewise an empty-Ok.
    let data = synthetic_db("CREATE TABLE whatever (id INTEGER);");
    let (sink, report) = run_plugin(&plugin, "db/docker.db", data);
    let report = report.expect("unknown panel db is not an error");
    assert_eq!(report.artifacts_found, 0);
    assert!(sink.artifacts.is_empty());
}

#[test]
fn corrupt_db_fails_closed_then_plugin_recovers() {
    let dir = bt_panel_plugin_util::stage_bt_panel_plugin();
    let plugin = load_bt_panel_plugin(dir.path());

    let mut corrupt = b"SQLite format 3\0".to_vec();
    corrupt.extend_from_slice(&[0xFFu8; 256]);
    let (sink, report) = run_plugin(&plugin, "default.db", corrupt);
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
    let data = synthetic_db(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, username TEXT, password TEXT);
        INSERT INTO users (username, password) VALUES ('ops', 'hash-material');",
    );
    let (sink, report) = run_plugin(&plugin, "default.db", data);
    assert_eq!(report.expect("plugin still works").artifacts_found, 1);
    assert_eq!(sink.artifacts[0].title, "ops");
}
