//! Real-data content-parse validation for the WeChat plugin: the databases
//! decrypted offline from the private Windows image (WeChat 4.1.8.67,
//! account `wxid_zuaa9igqlro22_eef8`) are fed to the real DLL as plaintext
//! bytes under their logical `db_storage` paths and asserted against the
//! known ground truth of that image:
//!
//! - `contact.db` → 27 WeChatContact artifacts, including
//!   `wxid_zuaa9igqlro22` (nickName 元气少女小倩) and `weixin`;
//! - `session.db` → 18 WeChatSession artifacts;
//! - `sns.db` → 6 WeChatMoment artifacts (plus moment timeline events);
//! - `favorite.db` → 0 WeChatFavorite artifacts (empty table, still Ok);
//! - `message_0.db` → 75 WeChatMessage artifacts across 17 distinct `Msg_`
//!   talker tables, every talker resolved via the Name2Id md5 reverse
//!   lookup, the earliest text message at unix 1774849642
//!   (`2026-03-30T05:47:22Z`), and at least one zstd-compressed row decoded.
//!
//! The decrypted bytes are private evidence derivatives: this test never
//! commits them, never hardcodes keys, and asserts only counts and
//! structural facts.
//!
//! Expected directory layout (the plaintext output of the offline
//! scan/decrypt tooling; this test only consumes it):
//!
//! ```text
//! <decrypted-dir>/
//!   contact.db  session.db  sns.db  favorite.db
//!   message_0.db  biz_message_0.db  message_fts.db  message_resource.db
//! ```
//!
//! Run:
//!   $env:FORENSICS_WECHAT_DECRYPTED_DIR='<decrypted-dir>'
//!   cargo test -p app-services --test wechat_content_realdata_validation -- --ignored --nocapture
#![cfg(windows)]

mod wechat_plugin_util;

use app_services::plugin_loader::{load_plugins_from_dirs, PluginExtractor};
use artifacts_core::{ArtifactContext, ArtifactExtractor, VecSink};
use domain::FileEntryId;
use std::collections::BTreeSet;

const WXID: &str = "wxid_zuaa9igqlro22_eef8";

fn decrypted_dir() -> std::path::PathBuf {
    std::env::var_os("FORENSICS_WECHAT_DECRYPTED_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            panic!("set FORENSICS_WECHAT_DECRYPTED_DIR to the decrypted-db directory")
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
    decrypted_dir: &std::path::Path,
    file: &str,
    logical_path: &str,
) -> (VecSink, artifacts_core::ExtractorReport) {
    let data = std::fs::read(decrypted_dir.join(file))
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

fn by_family<'a>(sink: &'a VecSink, family: &str) -> Vec<&'a domain::Artifact> {
    sink.artifacts
        .iter()
        .filter(|a| a.family == family)
        .collect()
}

#[test]
#[ignore = "requires decrypted WeChat databases from the private Windows image"]
fn real_decrypted_dbs_match_ground_truth() {
    let (_dir, plugin) = load_plugin();
    let decrypted = decrypted_dir();

    // contact.db: 27 contacts, including the owner account itself and the
    // WeChat team system account.
    let (sink, report) = run_real_file(
        &plugin,
        &decrypted,
        "contact.db",
        &db_path("contact", "contact.db"),
    );
    assert!(
        report.errors.is_empty(),
        "contact warnings: {:?}",
        report.errors
    );
    let contacts = by_family(&sink, "WeChatContact");
    assert_eq!(contacts.len(), 27, "contact.db ground truth is 27 rows");
    assert_eq!(by_family(&sink, "WeChatDatabase").len(), 1);
    let owner = contacts
        .iter()
        .find(|a| a.attrs["username"] == "wxid_zuaa9igqlro22")
        .expect("owner contact present");
    assert_eq!(owner.attrs["nickName"], "元气少女小倩");
    assert!(contacts.iter().any(|a| a.attrs["username"] == "weixin"));

    // session.db: 18 sessions, every one carrying unreadCount.
    let (sink, report) = run_real_file(
        &plugin,
        &decrypted,
        "session.db",
        &db_path("session", "session.db"),
    );
    assert!(
        report.errors.is_empty(),
        "session warnings: {:?}",
        report.errors
    );
    let sessions = by_family(&sink, "WeChatSession");
    assert_eq!(sessions.len(), 18, "session.db ground truth is 18 rows");
    assert!(sessions.iter().all(|a| a.attrs.contains_key("unreadCount")));
    assert!(sessions.iter().any(|a| a.attrs["username"] == "weixin"));

    // sns.db: 6 moments with timeline events; the known post at unix
    // 1774857283 is present.
    let (sink, report) = run_real_file(&plugin, &decrypted, "sns.db", &db_path("sns", "sns.db"));
    assert!(
        report.errors.is_empty(),
        "sns warnings: {:?}",
        report.errors
    );
    let moments = by_family(&sink, "WeChatMoment");
    assert_eq!(moments.len(), 6, "sns.db ground truth is 6 rows");
    assert!(moments
        .iter()
        .any(|a| a.attrs["createTimeUtc"] == "2026-03-30T07:54:43Z"));
    assert!(moments.iter().all(|a| a.attrs.contains_key("hasMedia")));
    assert!(report.timeline_events > 0, "moments emit timeline events");

    // favorite.db: table exists but holds zero rows on this image.
    let (sink, report) = run_real_file(
        &plugin,
        &decrypted,
        "favorite.db",
        &db_path("favorite", "favorite.db"),
    );
    assert!(
        report.errors.is_empty(),
        "favorite warnings: {:?}",
        report.errors
    );
    assert_eq!(by_family(&sink, "WeChatDatabase").len(), 1);
    assert_eq!(by_family(&sink, "WeChatFavorite").len(), 0);

    // message_0.db: 75 messages over 17 Msg_ tables; every talker resolved
    // through the Name2Id md5 reverse lookup; the earliest text message is
    // at unix 1774849642; zstd-compressed rows decode.
    let (sink, report) = run_real_file(
        &plugin,
        &decrypted,
        "message_0.db",
        &db_path("message", "message_0.db"),
    );
    assert!(
        report.errors.is_empty(),
        "message warnings: {:?}",
        report.errors
    );
    let messages = by_family(&sink, "WeChatMessage");
    assert_eq!(
        messages.len(),
        75,
        "message_0.db ground truth is 75 messages"
    );
    let tables: BTreeSet<&str> = messages
        .iter()
        .filter_map(|a| a.attrs["talkerTable"].as_str())
        .collect();
    assert_eq!(
        tables.len(),
        17,
        "message_0.db ground truth is 17 Msg_ tables"
    );
    assert!(
        messages.iter().all(|a| a.attrs.contains_key("talker")),
        "every talker table suffix resolves through Name2Id on this image"
    );
    let earliest_text = messages
        .iter()
        .filter(|a| a.attrs["localType"] == 1)
        .filter_map(|a| a.attrs["createTimeUtc"].as_str())
        .min()
        .expect("at least one text message");
    assert_eq!(earliest_text, "2026-03-30T05:47:22Z");
    let compressed: Vec<_> = messages
        .iter()
        .filter(|a| a.attrs["zstdCompressed"] == true)
        .collect();
    assert!(!compressed.is_empty(), "message_0.db has zstd rows");
    assert!(compressed
        .iter()
        .all(|a| a.attrs.contains_key("contentText")));
    // Direction resolution runs against the path-derived owner wxid.
    assert!(messages.iter().any(|a| a.attrs["isSend"] == true));
    assert!(messages.iter().any(|a| a.attrs["isSend"] == false));
    assert_eq!(report.timeline_events, 75, "one timeline event per message");

    // biz_message_0.db parses through the same message path; the fts and
    // resource companions stay schema-inventory only.
    let (sink, report) = run_real_file(
        &plugin,
        &decrypted,
        "biz_message_0.db",
        &db_path("message", "biz_message_0.db"),
    );
    assert!(
        report.errors.is_empty(),
        "biz message warnings: {:?}",
        report.errors
    );
    assert_eq!(by_family(&sink, "WeChatDatabase").len(), 1);
    assert!(!by_family(&sink, "WeChatMessage").is_empty());

    for file in ["message_fts.db", "message_resource.db"] {
        let (sink, report) = run_real_file(&plugin, &decrypted, file, &db_path("message", file));
        // FTS index tables use WCDB's custom tokenizer, which stock SQLite
        // cannot query; row-count warnings for those tables are expected.
        assert!(
            report
                .errors
                .iter()
                .all(|e| e.contains("row count on") || e.contains("tokenizer")),
            "{file} unexpected warnings: {:?}",
            report.errors
        );
        assert_eq!(by_family(&sink, "WeChatDatabase").len(), 1, "{file}");
        assert!(by_family(&sink, "WeChatMessage").is_empty(), "{file}");
        // Plaintext deep-parse still inventories the schema.
        assert!(sink.artifacts[0].attrs.contains_key("tableList"), "{file}");
    }
}
