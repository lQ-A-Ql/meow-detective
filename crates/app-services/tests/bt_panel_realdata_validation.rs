//! Real-sample validation for the BT Panel plugin: the databases extracted
//! from the private `web_dev_nvme0n1.E01` image (see
//! `tmp_bt_extract.rs`, output under `target/tmp/bt-panel-extract/`) are
//! driven across the real DLL boundary and asserted against the known
//! ground truth of that image.
//!
//! Run:
//!   $env:FORENSICS_BT_PANEL_EXTRACT_DIR='D:\process\forensic\target\tmp\bt-panel-extract'
//!   cargo test -p app-services --test bt_panel_realdata_validation -- --ignored --nocapture
#![cfg(windows)]

mod bt_panel_plugin_util;

use app_services::plugin_loader::{load_plugins_from_dirs, PluginExtractor};
use artifacts_core::{ArtifactContext, ArtifactExtractor, VecSink};
use domain::FileEntryId;
use rusqlite::Connection;

fn extract_dir() -> std::path::PathBuf {
    std::env::var_os("FORENSICS_BT_PANEL_EXTRACT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            panic!("set FORENSICS_BT_PANEL_EXTRACT_DIR to the bt-panel-extract directory")
        })
}

fn load_plugin() -> (tempfile::TempDir, PluginExtractor) {
    let dir = bt_panel_plugin_util::stage_bt_panel_plugin();
    let plugins = load_plugins_from_dirs(&[dir.path().to_path_buf()]);
    assert_eq!(plugins.len(), 1, "exactly one plugin DLL staged");
    let plugin = plugins.into_iter().next().expect("one plugin");
    assert_eq!(plugin.id(), "meow.plugin.bt_panel");
    (dir, plugin)
}

fn run_real_db(
    plugin: &PluginExtractor,
    extract_dir: &std::path::Path,
    file: &str,
    logical_path: &str,
) -> (VecSink, artifacts_core::ExtractorReport) {
    let data = std::fs::read(extract_dir.join(file))
        .unwrap_or_else(|error| panic!("read {file}: {error}"));
    let ctx = ArtifactContext {
        file_id: FileEntryId(format!("ds:1:{file}")),
        file_path: format!("[P2]/www/server/panel/data/{logical_path}"),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = plugin
        .run(ctx, &mut sink)
        .unwrap_or_else(|error| panic!("plugin run on {file} failed: {error}"));
    (sink, report)
}

#[test]
#[ignore = "requires extracted web_dev_nvme0n1 panel databases"]
fn real_panel_dbs_match_ground_truth() {
    let (_dir, plugin) = load_plugin();
    let extract = extract_dir();

    // default.db: factory-template admin (legacy, no salt column), 1 site,
    // 4 legacy firewall port rules; databases/ftps/crontab/logs are empty.
    let (sink, report) = run_real_db(&plugin, &extract, "default.db", "default.db");
    assert_eq!(report.artifacts_found, 6, "account+site+4 firewall");
    let account = sink
        .artifacts
        .iter()
        .find(|a| a.family == "BtPanelAccount")
        .expect("account artifact");
    assert_eq!(account.title, "admin");
    assert_eq!(account.attrs["hasPasswordHash"], true);
    assert!(
        account.attrs["passwordAlgorithm"]
            .as_str()
            .expect("algorithm")
            .contains("legacy"),
        "default.db users has no salt column: {:?}",
        account.attrs["passwordAlgorithm"]
    );
    assert_eq!(
        sink.artifacts
            .iter()
            .filter(|a| a.family == "BtPanelFirewall")
            .count(),
        4
    );

    // db/panel.db: the real login account, BT-0x proprietary format.
    let (sink, report) = run_real_db(&plugin, &extract, "db__panel.db", "db/panel.db");
    assert_eq!(report.artifacts_found, 1);
    let account = &sink.artifacts[0];
    assert_eq!(account.family, "BtPanelAccount");
    assert_eq!(account.title, "eswr2ymq");
    assert_eq!(account.attrs["hasPasswordHash"], true);
    assert_eq!(
        account.attrs["passwordAlgorithm"],
        "BT-0x proprietary (panel 9.x format)"
    );
    let dump = serde_json::to_string(&sink.artifacts).expect("dump");
    assert!(
        !dump.contains("hMAW4hiSRDp90jJkKGOo1p5kGWs9EpXci"),
        "password material must not cross the boundary"
    );

    // db/site.db: one site (tuoxin.com), no domain rows.
    let (sink, report) = run_real_db(&plugin, &extract, "db__site.db", "db/site.db");
    assert_eq!(report.artifacts_found, 1);
    assert_eq!(sink.artifacts[0].title, "tuoxin.com");
    assert_eq!(sink.artifacts[0].attrs["path"], "/www/wwwroot/tuoxin.com");
    assert_eq!(sink.artifacts[0].attrs["statusText"], "running");

    // db/database.db: three databases, passwords redacted.
    let (sink, report) = run_real_db(&plugin, &extract, "db__database.db", "db/database.db");
    assert_eq!(report.artifacts_found, 3);
    let names: Vec<&str> = sink.artifacts.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(names, ["tuoxin_mall", "test", "jelly"]);
    for artifact in &sink.artifacts {
        // Ground truth: tuoxin_mall has an empty password; test/jelly have
        // real secrets (30/50 chars).
        let expected = artifact.title != "tuoxin_mall";
        assert_eq!(
            artifact.attrs["hasPassword"],
            serde_json::Value::Bool(expected),
            "hasPassword for {}: {:?}",
            artifact.title,
            artifact.attrs["hasPassword"]
        );
    }
    // Redaction against the real secret values: read them from the source
    // bytes and assert they never appear in the sink dump.
    let dump = serde_json::to_string(&sink.artifacts).expect("dump");
    let raw = std::fs::read(extract.join("db__database.db")).expect("raw db");
    let mut conn = Connection::open_in_memory().expect("mem");
    conn.deserialize(
        rusqlite::DatabaseName::Main,
        unsafe {
            // SAFETY: `sqlite3_malloc` returns SQLite-owned memory which
            // `OwnedData` frees with `sqlite3_free` (FREEONCLOSE after
            // deserialize); the raw bytes are fully copied in.
            let ptr = rusqlite::ffi::sqlite3_malloc(raw.len() as i32);
            assert!(!ptr.is_null());
            std::ptr::copy_nonoverlapping(raw.as_ptr(), ptr.cast::<u8>(), raw.len());
            rusqlite::serialize::OwnedData::from_raw_nonnull(
                std::ptr::NonNull::new(ptr.cast::<u8>()).expect("non-null"),
                raw.len(),
            )
        },
        true,
    )
    .expect("deserialize");
    let mut stmt = conn
        .prepare("SELECT password FROM databases")
        .expect("prepare");
    let passwords: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows");
    for password in passwords.iter().filter(|p| !p.is_empty()) {
        assert!(
            !dump.contains(password.as_str()),
            "database password material must not cross the boundary"
        );
    }

    // db/ftp.db: empty ftps table — zero artifacts, no error.
    let (sink, report) = run_real_db(&plugin, &extract, "db__ftp.db", "db/ftp.db");
    assert_eq!(report.artifacts_found, 0);
    assert!(sink.artifacts.is_empty());

    // db/firewall.db: 11 legacy port rules + 4 firewall_new rules.
    let (_sink, report) = run_real_db(&plugin, &extract, "db__firewall.db", "db/firewall.db");
    assert_eq!(report.artifacts_found, 15);

    // db/crontab.db: one Let's Encrypt renewal task with its command body.
    let (sink, report) = run_real_db(&plugin, &extract, "db__crontab.db", "db/crontab.db");
    assert_eq!(report.artifacts_found, 1);
    let task = &sink.artifacts[0];
    assert_eq!(task.family, "BtPanelTask");
    assert_eq!(task.attrs["cycleType"], "day");
    assert!(
        task.attrs["command"]
            .as_str()
            .expect("command")
            .contains("acme_v2.py"),
        "crontab sBody must surface: {:?}",
        task.attrs["command"]
    );

    // db/log.db: 64 operation rows → 64 artifacts + 64 timeline events.
    let (sink, report) = run_real_db(&plugin, &extract, "db__log.db", "db/log.db");
    assert_eq!(report.artifacts_found, 64);
    assert_eq!(report.timeline_events, 64);
    assert_eq!(sink.timeline_events.len(), 64);
    assert!(sink
        .timeline_events
        .iter()
        .all(|e| e.event_type == "BT_PANEL_OPERATION"));
    assert!(
        sink.artifacts.iter().any(|a| a.title.contains("用户登录")),
        "login log rows present"
    );
}
