//! Real-data WAL-merge validation for the WeChat plugin: the encrypted
//! databases and their un-checkpointed `.db-wal` companions from the
//! private qianqian image (WeChat 4.1.8.67, account
//! `wxid_zuaa9igqlro22_eef8`) are merged offline through the plugin crate's
//! `wechat_offline` developer binary (`sqlcipher4` + `walmerge`) and the
//! merged plaintext images are asserted:
//!
//! - `message_0.db` merged with `message_0.db-wal` keeps a valid
//!   `PRAGMA integrity_check` and yields at least as many message rows as
//!   the checkpointed-only baseline in `decrypted/message_0.db` (75 on the
//!   reference snapshot, where every WAL frame was already checkpointed —
//!   the merge is content-neutral there; a snapshot taken while WeChat had
//!   un-checkpointed writes would yield strictly more);
//! - `contact` / `session` / `sns` merged images open and pass
//!   `integrity_check` as well.
//!
//! Inputs (never committed, never hardcoded here):
//!
//! ```text
//! <extract-dir>/                       parent of FORENSICS_WECHAT_DECRYPTED_DIR
//!   message_0.db  message_0.db-wal  contact.db  contact.db-wal  ...
//!   decrypted/                        = FORENSICS_WECHAT_DECRYPTED_DIR
//!     message_0.db  contact.db  ...   (checkpointed-only baselines)
//! <keys.json>                          = FORENSICS_WECHAT_TEST_KEYS
//!   {"message_0.db": "<64-hex>", ...}
//! ```
//!
//! Run:
//!   $env:FORENSICS_WECHAT_DECRYPTED_DIR='<extract-dir>/decrypted'
//!   $env:FORENSICS_WECHAT_TEST_KEYS='<keys.json>'
//!   cargo test -p app-services --test wechat_walmerge_realdata_validation -- --ignored --nocapture
//!
//! Both environment variables are optional: when either is missing the
//! test prints the reason and returns without asserting (never a hard
//! failure on machines without the private evidence).
#![cfg(windows)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Message-table row total across every `Msg_*` table in a plaintext db.
fn message_rows(db: &Path) -> i64 {
    let conn = rusqlite::Connection::open(db)
        .unwrap_or_else(|error| panic!("open {}: {error}", db.display()));
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap_or_else(|error| panic!("integrity_check {}: {error}", db.display()));
    assert_eq!(integrity, "ok", "{} integrity", db.display());
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'Msg\\_%' ESCAPE '\\'")
        .expect("msg table query");
    let tables: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .expect("msg tables")
        .collect::<Result<_, _>>()
        .expect("msg table rows");
    assert!(!tables.is_empty(), "{} has no Msg_ tables", db.display());
    let mut total = 0;
    for table in tables {
        let escaped = table.replace('"', "\"\"");
        let count: i64 = conn
            .query_row(&format!("SELECT count(*) FROM \"{escaped}\""), [], |row| {
                row.get(0)
            })
            .unwrap_or_else(|error| panic!("count {table}: {error}"));
        total += count;
    }
    total
}

/// Open a merged db and require a clean integrity check; returns the total
/// row count over all user tables (schema-agnostic smoke metric).
fn merged_rows_and_integrity(db: &Path) -> i64 {
    let conn = rusqlite::Connection::open(db)
        .unwrap_or_else(|error| panic!("open {}: {error}", db.display()));
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap_or_else(|error| panic!("integrity_check {}: {error}", db.display()));
    assert_eq!(integrity, "ok", "{} integrity", db.display());
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")
        .expect("table query");
    let tables: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .expect("tables")
        .collect::<Result<_, _>>()
        .expect("table rows");
    let mut total = 0;
    for table in tables {
        let escaped = table.replace('"', "\"\"");
        // WCDB FTS shadow tables use a custom tokenizer that stock SQLite
        // cannot read; those tables are out of scope for this metric.
        let count: Result<i64, _> =
            conn.query_row(&format!("SELECT count(*) FROM \"{escaped}\""), [], |row| {
                row.get(0)
            });
        if let Ok(count) = count {
            total += count;
        }
    }
    total
}

/// Build the plugins-src workspace (same target dir as
/// `wechat_plugin_util`) and return the `wechat_offline` binary path.
fn wechat_offline_exe() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = manifest_dir.join("../../plugins-src/Cargo.toml");
    let target = manifest_dir.join("../../target/plugins-src");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = std::process::Command::new(cargo)
        .arg("build")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--bin")
        .arg("wechat_offline")
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .expect("spawn cargo build for wechat_offline");
    assert!(status.success(), "wechat_offline build failed");
    target.join("debug/wechat_offline.exe")
}

fn read_keys(keys_path: &Path) -> BTreeMap<String, String> {
    let data = std::fs::read(keys_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", keys_path.display()));
    serde_json::from_slice(&data)
        .unwrap_or_else(|error| panic!("parse {}: {error}", keys_path.display()))
}

#[test]
#[ignore = "requires the private qianqian-image WeChat databases, WALs, and keys"]
fn wal_merged_dbs_exceed_checkpointed_baseline() {
    let Some(decrypted) = std::env::var_os("FORENSICS_WECHAT_DECRYPTED_DIR").map(PathBuf::from)
    else {
        eprintln!("SKIP: set FORENSICS_WECHAT_DECRYPTED_DIR to the decrypted-db directory");
        return;
    };
    let Some(keys_path) = std::env::var_os("FORENSICS_WECHAT_TEST_KEYS").map(PathBuf::from) else {
        eprintln!("SKIP: set FORENSICS_WECHAT_TEST_KEYS to the local keys.json");
        return;
    };
    let extract = decrypted
        .parent()
        .unwrap_or_else(|| panic!("{decrypted:?} has no parent directory"))
        .to_path_buf();
    let keys = read_keys(&keys_path);
    let exe = wechat_offline_exe();
    let out_dir = tempfile::tempdir().expect("temp output dir");

    let merge = |name: &str| -> PathBuf {
        let db = extract.join(name);
        let wal = extract.join(format!("{name}-wal"));
        let out = out_dir.path().join(format!("merged-{name}"));
        let key = keys
            .get(name)
            .unwrap_or_else(|| panic!("keys.json has no entry for {name}"));
        let output = std::process::Command::new(&exe)
            .arg("decrypt")
            .arg(key)
            .arg(&db)
            .arg(&out)
            .arg(&wal)
            .output()
            .expect("run wechat_offline decrypt");
        assert!(
            output.status.success(),
            "wechat_offline merge of {name} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        eprintln!("{name}: {}", String::from_utf8_lossy(&output.stdout));
        out
    };

    // message_0: merged rows must never regress below the checkpointed
    // baseline. On the reference snapshot the WAL frames were already
    // checkpointed into the main db, so merged == baseline; a live-capture
    // snapshot with un-checkpointed writes would exceed it.
    let baseline_db = decrypted.join("message_0.db");
    let baseline_rows = message_rows(&baseline_db);
    assert_eq!(baseline_rows, 75, "checkpointed ground truth is 75 rows");
    let merged_db = merge("message_0.db");
    let merged_rows = message_rows(&merged_db);
    eprintln!("message_0 rows: baseline={baseline_rows} merged={merged_rows}");
    assert!(
        merged_rows >= baseline_rows,
        "merged message_0 rows {merged_rows} regressed below the checkpointed baseline {baseline_rows}"
    );

    // contact/session/sns: merged images open cleanly; row totals must
    // never regress below the checkpointed baselines.
    for (name, table) in [
        ("contact.db", "contact"),
        ("session.db", "SessionTable"),
        ("sns.db", "SnsTimeLine"),
    ] {
        let merged_db = merge(name);
        let total = merged_rows_and_integrity(&merged_db);
        let baseline_conn = rusqlite::Connection::open(decrypted.join(name))
            .unwrap_or_else(|error| panic!("open baseline {name}: {error}"));
        let baseline: i64 = baseline_conn
            .query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |row| {
                row.get(0)
            })
            .unwrap_or_else(|error| panic!("baseline count {name}: {error}"));
        let merged_conn = rusqlite::Connection::open(&merged_db)
            .unwrap_or_else(|error| panic!("reopen merged {name}: {error}"));
        let merged: i64 = merged_conn
            .query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |row| {
                row.get(0)
            })
            .unwrap_or_else(|error| panic!("merged count {name}: {error}"));
        eprintln!(
            "{name}: baseline {table}={baseline} merged={merged} (all-user-table rows {total})"
        );
        assert!(
            merged >= baseline,
            "{name} merged {table} rows {merged} regressed below baseline {baseline}"
        );
    }
}
