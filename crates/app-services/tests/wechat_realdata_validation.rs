//! Real-sample validation for the WeChat plugin: the files extracted from
//! the private Windows image (WeChat 4.1.8.67, account
//! `wxid_zuaa9igqlro22_eef8`) are driven across the real DLL boundary and
//! asserted against the known ground truth of that image: every
//! `db_storage` database is WCDB/SQLCipher-encrypted, so the plugin must
//! inventory them (wxid/category/name/size + `encrypted=true`) and attach
//! the offline-unrecoverable warning, while the plaintext side files parse
//! fully.
//!
//! Expected extract directory layout (produced from the E01 by
//! `examples/extract_wechat.rs`; this test only consumes it):
//!
//! ```text
//! <extract-dir>/
//!   plugin_info.ini        Program Files/Tencent/Weixin/4.1.8.67/plugin_info.ini
//!   cloud_account.txt      .../xwechat/ilink/wechat/cloud_account.txt
//!   key_info.dat           .../xwechat/login/wxid_zuaa9igqlro22/key_info.dat
//!   kvcomm_config.ini      .../xwechat/ilink/kvcomm/config.ini
//!   contact.db             db_storage/contact/contact.db
//!   session.db             db_storage/session/session.db
//!   sns.db                 db_storage/sns/sns.db
//!   favorite.db            db_storage/favorite/favorite.db
//!   message_0.db           db_storage/message/message_0.db
//!   biz_message_0.db       db_storage/message/biz_message_0.db
//!   message_fts.db         db_storage/message/message_fts.db
//!   message_resource.db    db_storage/message/message_resource.db
//! ```
//!
//! Run:
//!   $env:FORENSICS_WECHAT_EXTRACT_DIR='target\tmp\wechat-extract'
//!   cargo test -p app-services --test wechat_realdata_validation -- --ignored --nocapture
#![cfg(windows)]

mod wechat_plugin_util;

use app_services::plugin_loader::{load_plugins_from_dirs, PluginExtractor};
use artifacts_core::{ArtifactContext, ArtifactExtractor, VecSink};
use domain::FileEntryId;

const WXID: &str = "wxid_zuaa9igqlro22_eef8";

fn extract_dir() -> std::path::PathBuf {
    std::env::var_os("FORENSICS_WECHAT_EXTRACT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            panic!("set FORENSICS_WECHAT_EXTRACT_DIR to the wechat-extract directory")
        })
}

fn load_plugin() -> (tempfile::TempDir, PluginExtractor) {
    let dir = wechat_plugin_util::stage_wechat_plugin();
    let plugins = load_plugins_from_dirs(&[dir.path().to_path_buf()]);
    assert_eq!(plugins.len(), 1, "exactly one plugin DLL staged");
    let plugin = plugins.into_iter().next().expect("one plugin");
    assert_eq!(plugin.id(), "meow.plugin.wechat");
    (dir, plugin)
}

fn run_real_file(
    plugin: &PluginExtractor,
    extract_dir: &std::path::Path,
    file: &str,
    logical_path: &str,
) -> (VecSink, artifacts_core::ExtractorReport) {
    let data = std::fs::read(extract_dir.join(file))
        .unwrap_or_else(|error| panic!("read {file}: {error}"));
    let ctx = ArtifactContext {
        file_id: FileEntryId(format!("ds:1:{file}")),
        file_path: logical_path.to_string(),
        reader: Box::new(std::io::Cursor::new(data)),
    };
    let mut sink = VecSink::new();
    let report = plugin
        .run(ctx, &mut sink)
        .unwrap_or_else(|error| panic!("plugin run on {file} failed: {error}"));
    (sink, report)
}

fn db_path(category: &str, name: &str) -> String {
    format!("[P2]/Users/admin/Documents/xwechat_files/{WXID}/db_storage/{category}/{name}")
}

#[test]
#[ignore = "requires extracted WeChat files from the private Windows image"]
fn real_wechat_files_match_ground_truth() {
    let (_dir, plugin) = load_plugin();
    let extract = extract_dir();

    // plugin_info.ini: install version 4.1.8.67 from the path, plugin
    // version table parsed from the INI body.
    let (sink, report) = run_real_file(
        &plugin,
        &extract,
        "plugin_info.ini",
        "[P2]/Program Files/Tencent/Weixin/4.1.8.67/plugin_info.ini",
    );
    assert_eq!(report.artifacts_found, 1);
    let install = &sink.artifacts[0];
    assert_eq!(install.family, "WeChatInstall");
    assert_eq!(install.attrs["installVersion"], "4.1.8.67");
    assert!(
        install.attrs["pluginVersions"]
            .as_object()
            .expect("pluginVersions")
            .len()
            > 0,
        "plugin_info.ini must yield at least one plugin version entry"
    );

    // cloud_account.txt: kTdiKeyCloudSession is empty on this image.
    let (sink, report) = run_real_file(
        &plugin,
        &extract,
        "cloud_account.txt",
        "[P2]/Users/admin/AppData/Roaming/Tencent/xwechat/ilink/wechat/cloud_account.txt",
    );
    assert_eq!(report.artifacts_found, 1);
    assert_eq!(sink.artifacts[0].family, "WeChatAccount");
    assert_eq!(sink.artifacts[0].attrs["hasCloudSession"], false);

    // key_info.dat: 180-byte encrypted login key material under the wxid
    // login directory; inventory only.
    let (sink, report) = run_real_file(
        &plugin,
        &extract,
        "key_info.dat",
        "[P2]/Users/admin/AppData/Roaming/Tencent/xwechat/login/wxid_zuaa9igqlro22/key_info.dat",
    );
    assert_eq!(report.artifacts_found, 1);
    let key_info = &sink.artifacts[0];
    assert_eq!(key_info.family, "WeChatAccount");
    assert_eq!(key_info.attrs["wxid"], "wxid_zuaa9igqlro22");
    assert_eq!(key_info.attrs["keyInfoPresent"], true);
    assert_eq!(key_info.attrs["sizeBytes"], 180);

    // kvcomm config.ini: settings object parsed.
    let (sink, report) = run_real_file(
        &plugin,
        &extract,
        "kvcomm_config.ini",
        "[P2]/Users/admin/AppData/Roaming/Tencent/xwechat/ilink/kvcomm/config.ini",
    );
    assert_eq!(report.artifacts_found, 1);
    assert_eq!(sink.artifacts[0].family, "WeChatInstall");

    // All db_storage databases are WCDB/SQLCipher-encrypted on this image:
    // inventory only, correct wxid/category, one warning per database.
    let databases: &[(&str, &str, &str)] = &[
        ("contact.db", "contact", "contact.db"),
        ("session.db", "session", "session.db"),
        ("sns.db", "sns", "sns.db"),
        ("favorite.db", "favorite", "favorite.db"),
        ("message_0.db", "message", "message_0.db"),
        ("biz_message_0.db", "message", "biz_message_0.db"),
        ("message_fts.db", "message", "message_fts.db"),
        ("message_resource.db", "message", "message_resource.db"),
    ];
    for (file, category, db_name) in databases {
        let (sink, report) = run_real_file(&plugin, &extract, file, &db_path(category, db_name));
        assert_eq!(report.artifacts_found, 1, "{file}");
        let artifact = &sink.artifacts[0];
        assert_eq!(artifact.family, "WeChatDatabase", "{file}");
        assert_eq!(artifact.attrs["encrypted"], true, "{file}");
        assert_eq!(artifact.attrs["wxid"], WXID, "{file}");
        assert_eq!(artifact.attrs["category"], *category, "{file}");
        assert_eq!(artifact.attrs["dbName"], *db_name, "{file}");
        assert!(
            artifact.attrs["sizeBytes"].as_u64().expect("sizeBytes") > 0,
            "{file}"
        );
        assert!(artifact.attrs.get("tableList").is_none(), "{file}");
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("WCDB/SQLCipher") && e.contains("无法离线恢复")),
            "{file} must carry the offline-unrecoverable warning: {:?}",
            report.errors
        );
        // Ground truth double-check: the source bytes really lack the
        // plaintext SQLite header.
        let raw = std::fs::read(extract.join(file)).expect("raw db");
        assert!(
            raw.len() < 16 || &raw[..16] != b"SQLite format 3\0",
            "{file} is unexpectedly plaintext on this image"
        );
    }
}
