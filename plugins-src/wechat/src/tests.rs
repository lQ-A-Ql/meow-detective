//! Plugin-internal unit tests: synthetic SQLite databases and side files
//! driven through the real exported functions (full request/response/free
//! cycle), covering every route, the path self-filter, corrupt-input
//! ParseError, and panic self-capture.

use super::*;
use rusqlite::Connection;
use serde_json::Value;
use std::ffi::CString;

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

/// Build a synthetic database populated through bound parameters (needed
/// for blob/zstd payloads that cannot live in a SQL literal).
fn synthetic_db_with(build: impl FnOnce(&Connection)) -> Vec<u8> {
    let conn = Connection::open_in_memory().expect("in-memory db");
    build(&conn);
    let data = conn
        .serialize(rusqlite::DatabaseName::Main)
        .expect("serialize fixture");
    data.to_vec()
}

fn artifacts_of(payload: &Value, family: &str) -> Vec<Value> {
    payload["artifacts"]
        .as_array()
        .expect("artifacts")
        .iter()
        .filter(|a| a["family"] == family)
        .cloned()
        .collect()
}

fn warning_texts(payload: &Value) -> Vec<String> {
    payload["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .map(|w| w.as_str().unwrap_or_default().to_string())
        .collect()
}

fn request_for<'a>(path: &'a CString, id: &'a CString, data: &'a [u8]) -> MeowExtractRequest {
    MeowExtractRequest {
        struct_size: std::mem::size_of::<MeowExtractRequest>() as u32,
        file_path: path.as_ptr().cast(),
        file_id: id.as_ptr().cast(),
        data: data.as_ptr(),
        data_len: data.len() as u64,
        companions: std::ptr::null(),
        companion_count: 0,
    }
}

/// Read and free the response buffers exactly the way the host does.
unsafe fn drain_response(response: &MeowExtractResponse) -> (Option<Vec<u8>>, Option<String>) {
    let payload = if response.payload.is_null() {
        None
    } else {
        // SAFETY: payload/payload_len were allocated by this plugin.
        let bytes =
            unsafe { std::slice::from_raw_parts(response.payload, response.payload_len as usize) }
                .to_vec();
        unsafe { meow_plugin_free_buffer(response.payload, response.payload_len) };
        Some(bytes)
    };
    let error = if response.error_message.is_null() {
        None
    } else {
        // SAFETY: error_message is a NUL-terminated plugin allocation.
        let (text, len) = unsafe {
            let message = CStr::from_ptr(response.error_message.cast());
            (
                message.to_string_lossy().into_owned(),
                message.to_bytes_with_nul().len() as u64,
            )
        };
        unsafe { meow_plugin_free_buffer(response.error_message, len) };
        Some(text)
    };
    (payload, error)
}

fn run_plugin(path_str: &str, data: &[u8]) -> (MeowStatus, Option<Value>, Option<String>) {
    let path = CString::new(path_str).expect("path");
    let id = CString::new("ds:1:wechat-1").expect("id");
    let request = request_for(&path, &id, data);
    let response = unsafe { meow_plugin_extract(&request) };
    let status = response.status;
    let (payload, error) = unsafe { drain_response(&response) };
    let payload = payload.map(|bytes| serde_json::from_slice(&bytes).expect("valid JSON"));
    (status, payload, error)
}

fn run_plugin_with_wal(path_str: &str, data: &[u8], wal: &[u8]) -> Value {
    let path = CString::new(path_str).expect("path");
    let id = CString::new("ds:1:wechat-1").expect("id");
    let wal_path = CString::new(format!("{path_str}-wal")).expect("wal path");
    let wal_id = CString::new("ds:1:wechat-wal-1").expect("wal id");
    let companion = MeowCompanionFile {
        struct_size: std::mem::size_of::<MeowCompanionFile>() as u32,
        file_path: wal_path.as_ptr().cast(),
        file_id: wal_id.as_ptr().cast(),
        data: wal.as_ptr(),
        data_len: wal.len() as u64,
    };
    let mut request = request_for(&path, &id, data);
    request.companions = &companion;
    request.companion_count = 1;
    let response = unsafe { meow_plugin_extract(&request) };
    assert_eq!(response.status, MeowStatus::Ok);
    let (payload, error) = unsafe { drain_response(&response) };
    assert!(error.is_none());
    serde_json::from_slice(&payload.expect("payload")).expect("valid JSON")
}

#[test]
fn info_reports_expected_metadata() {
    let info = unsafe { meow_plugin_info() };
    assert_eq!(info.abi_version, MEOW_PLUGIN_ABI_VERSION);
    assert_eq!(
        info.struct_size,
        std::mem::size_of::<MeowPluginInfo>() as u32
    );
    assert_eq!(info.evidence_platform, MeowEvidencePlatform::Windows);
    unsafe {
        assert_eq!(CStr::from_ptr(info.plugin_id.cast()), c"meow.plugin.wechat");
        assert_eq!(CStr::from_ptr(info.plugin_version.cast()), c"0.4.2");
        assert_eq!(CStr::from_ptr(info.display_name.cast()), c"微信");
        let families: Value = serde_json::from_str(
            CStr::from_ptr(info.families_json.cast())
                .to_str()
                .expect("families utf8"),
        )
        .expect("families json");
        assert_eq!(
            families,
            serde_json::json!([
                "WeChatInstall",
                "WeChatAccount",
                "WeChatDatabase",
                "WeChatContact",
                "WeChatMessage",
                "WeChatSession",
                "WeChatMoment",
                "WeChatFavorite",
                "WeChatMedia",
                "WeChatSearchRecord"
            ])
        );
        let patterns: Value = serde_json::from_str(
            CStr::from_ptr(info.path_patterns_json.cast())
                .to_str()
                .expect("patterns utf8"),
        )
        .expect("patterns json");
        assert_eq!(
            patterns,
            serde_json::json!([
                "*.db",
                "plugin_info.ini",
                "cloud_account.txt",
                "key_info.dat",
                "config.ini",
                "/msg/attach/",
                "/sns/img/"
            ])
        );
    }
}

#[test]
fn panic_inside_extract_is_self_caught() {
    let request = MeowExtractRequest {
        struct_size: std::mem::size_of::<MeowExtractRequest>() as u32,
        file_path: std::ptr::null(),
        file_id: std::ptr::null(),
        data: std::ptr::null(),
        data_len: 0,
        companions: std::ptr::null(),
        companion_count: 0,
    };
    let response = unsafe { guarded_extract(&request, |_| panic!("boom")) };
    assert_eq!(response.status, MeowStatus::InternalError);
    let (_, error) = unsafe { drain_response(&response) };
    assert!(error.expect("error message").contains("panicked"));
}

#[test]
fn null_request_fails_closed() {
    let response = unsafe { meow_plugin_extract(std::ptr::null()) };
    assert_eq!(response.status, MeowStatus::InternalError);
    let (_, error) = unsafe { drain_response(&response) };
    assert!(error.is_some());
}

#[test]
fn non_wechat_db_path_is_silent_ok() {
    // The host's "*.db" filter is wide; the plugin self-filter must reject
    // foreign databases without an error and without parsing.
    let (status, payload, error) = run_plugin("[P0]/var/lib/mysql/mysql/user.db", b"garbage");
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    assert_eq!(payload["artifacts"].as_array().expect("artifacts").len(), 0);
    assert_eq!(payload["timesAreLocal"], false);
}

#[test]
fn xwechat_db_outside_db_storage_is_silent_ok() {
    let path = "[P2]/Users/admin/Documents/xwechat_files/wxid_zuaa9igqlro22_eef8/backup/note.db";
    let (status, payload, error) = run_plugin(path, b"garbage");
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    assert_eq!(
        payload.expect("payload")["artifacts"]
            .as_array()
            .expect("artifacts")
            .len(),
        0
    );
}

#[test]
fn unrelated_side_file_names_are_silent_ok() {
    for path in [
        "[P2]/Users/admin/AppData/Roaming/other/plugin_info.ini",
        "[P2]/Users/admin/AppData/Roaming/other/cloud_account.txt",
        "[P2]/Windows/key_info.dat",
        "[P2]/etc/config.ini",
    ] {
        let (status, payload, error) = run_plugin(path, b"k=v");
        assert_eq!(status, MeowStatus::Ok, "{path}");
        assert!(error.is_none(), "{path}");
        assert_eq!(
            payload.expect("payload")["artifacts"]
                .as_array()
                .expect("artifacts")
                .len(),
            0,
            "{path}"
        );
    }
}

#[test]
fn plaintext_database_is_deep_parsed() {
    let data = synthetic_db(
        "CREATE TABLE message (id INTEGER PRIMARY KEY, content TEXT);
        INSERT INTO message (content) VALUES ('hello'), ('world'), ('again');
        CREATE TABLE contact (id INTEGER PRIMARY KEY, username TEXT);
        INSERT INTO contact (username) VALUES ('wxid_a'), ('wxid_b');",
    );
    let path = format!("{DATA_PREFIX}/message/message_0.db");
    let (status, payload, error) = run_plugin(&path, &data);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    let artifact = &payload["artifacts"][0];
    assert_eq!(artifact["family"], "WeChatDatabase");
    assert_eq!(artifact["title"], "message/message_0.db");
    assert_eq!(artifact["attrs"]["wxid"], "wxid_zuaa9igqlro22_eef8");
    assert_eq!(artifact["attrs"]["category"], "message");
    assert_eq!(artifact["attrs"]["dbName"], "message_0.db");
    assert_eq!(artifact["attrs"]["encrypted"], false);
    assert_eq!(artifact["attrs"]["sizeBytes"], data.len() as u64);
    assert_eq!(artifact["attrs"]["tableCount"], 2);
    assert_eq!(
        artifact["attrs"]["tableList"],
        serde_json::json!(["contact", "message"])
    );
    assert_eq!(artifact["attrs"]["rowCounts"]["message"], 3);
    assert_eq!(artifact["attrs"]["rowCounts"]["contact"], 2);
    assert!(payload["warnings"].as_array().expect("warnings").is_empty());
}

#[test]
fn database_request_accepts_wal_companion() {
    let data = synthetic_db("CREATE TABLE message (id INTEGER PRIMARY KEY);");
    let path = format!("{DATA_PREFIX}/message/message_0.db");
    let payload = run_plugin_with_wal(&path, &data, &[]);
    let database = artifacts_of(&payload, "WeChatDatabase");
    assert_eq!(database[0]["attrs"]["walPresent"], true);
}

#[test]
fn encrypted_database_is_inventory_only_with_warning() {
    // Deterministic pseudo-random bytes with a non-SQLite header.
    let data: Vec<u8> = (0..512u32)
        .map(|i| (i.wrapping_mul(2654435761) % 251) as u8)
        .collect();
    let path = format!("{DATA_PREFIX}/contact/contact.db");
    let (status, payload, error) = run_plugin(&path, &data);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    let artifact = &payload["artifacts"][0];
    assert_eq!(artifact["family"], "WeChatDatabase");
    assert_eq!(artifact["attrs"]["encrypted"], true);
    assert_eq!(artifact["attrs"]["category"], "contact");
    assert_eq!(artifact["attrs"]["wxid"], "wxid_zuaa9igqlro22_eef8");
    assert!(artifact["attrs"].get("tableList").is_none());
    let warnings = payload["warnings"].as_array().expect("warnings");
    assert!(warnings
        .iter()
        .any(|w| w.as_str().unwrap_or_default().contains("WCDB/SQLCipher")));
}

#[test]
fn truncated_input_never_panics() {
    let path = format!("{DATA_PREFIX}/session/session.db");
    for data in [&b""[..], &b"SQL"[..], &b"SQLite format 3\0"[..]] {
        let (status, payload, error) = run_plugin(&path, data);
        // Short inputs are inventoried as encrypted; the exact 16-byte
        // header alone is a broken plaintext database → ParseError, but
        // either way the plugin must answer, never unwind.
        match status {
            MeowStatus::Ok => {
                assert!(error.is_none());
                assert!(payload.is_some());
            }
            MeowStatus::ParseError => assert!(error.is_some()),
            other => panic!("unexpected status {other:?}"),
        }
    }
}

#[test]
fn corrupt_plaintext_database_is_parse_error() {
    let mut data = b"SQLite format 3\0".to_vec();
    data.extend_from_slice(&[0xFFu8; 256]);
    let path = format!("{DATA_PREFIX}/sns/sns.db");
    let (status, _, error) = run_plugin(&path, &data);
    assert_eq!(status, MeowStatus::ParseError);
    assert!(error.is_some());
}

#[test]
fn plugin_info_ini_yields_install_artifact() {
    let ini = b"\xef\xbb\xbf[plugin]\r\nWeChatPlayer=1.0.0.12\r\nFinder=2.3.4.5\r\n; comment\r\n";
    let path = "[P2]/Program Files/Tencent/Weixin/4.1.8.67/plugin_info.ini";
    let (status, payload, error) = run_plugin(path, ini);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    let artifact = &payload["artifacts"][0];
    assert_eq!(artifact["family"], "WeChatInstall");
    assert_eq!(artifact["title"], "微信 4.1.8.67");
    assert_eq!(artifact["attrs"]["installVersion"], "4.1.8.67");
    assert_eq!(
        artifact["attrs"]["installPath"],
        "[P2]/Program Files/Tencent/Weixin/4.1.8.67"
    );
    assert_eq!(
        artifact["attrs"]["pluginVersions"]["plugin.WeChatPlayer"],
        "1.0.0.12"
    );
    assert_eq!(
        artifact["attrs"]["pluginVersions"]["plugin.Finder"],
        "2.3.4.5"
    );
}

#[test]
fn cloud_account_reports_presence_only() {
    let path = "[P2]/Users/admin/AppData/Roaming/Tencent/xwechat/ilink/wechat/cloud_account.txt";
    let (status, payload, _) = run_plugin(path, b"kTdiKeyCloudSession=\r\n");
    assert_eq!(status, MeowStatus::Ok);
    let artifact = &payload.expect("payload")["artifacts"][0];
    assert_eq!(artifact["family"], "WeChatAccount");
    assert_eq!(artifact["attrs"]["hasCloudSession"], false);

    let secret = "kTdiKeyCloudSession=super-secret-session-token";
    let (status, payload, _) = run_plugin(path, secret.as_bytes());
    assert_eq!(status, MeowStatus::Ok);
    let payload = payload.expect("payload");
    assert_eq!(payload["artifacts"][0]["attrs"]["hasCloudSession"], true);
    // Redaction: the session value must never cross the boundary.
    assert!(!payload.to_string().contains("super-secret-session-token"));
}

#[test]
fn key_info_dat_is_inventory_only() {
    let blob: Vec<u8> = (0..180u32).map(|i| (i % 256) as u8).collect();
    let path =
        "[P2]/Users/admin/AppData/Roaming/Tencent/xwechat/login/wxid_zuaa9igqlro22/key_info.dat";
    let (status, payload, error) = run_plugin(path, &blob);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let artifact = &payload.expect("payload")["artifacts"][0];
    assert_eq!(artifact["family"], "WeChatAccount");
    assert_eq!(artifact["attrs"]["wxid"], "wxid_zuaa9igqlro22");
    assert_eq!(artifact["attrs"]["keyInfoPresent"], true);
    assert_eq!(artifact["attrs"]["sizeBytes"], 180);
    assert!(artifact["summary"]
        .as_str()
        .unwrap_or_default()
        .contains("已加密"));
}

#[test]
fn kvcomm_config_ini_yields_install_artifact() {
    let ini = b"kv_clientversion=41080067\r\nkv_ticket=\r\n";
    let path = "[P2]/Users/admin/AppData/Roaming/Tencent/xwechat/ilink/kvcomm/config.ini";
    let (status, payload, error) = run_plugin(path, ini);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let artifact = &payload.expect("payload")["artifacts"][0];
    assert_eq!(artifact["family"], "WeChatInstall");
    assert_eq!(
        artifact["attrs"]["settings"]["kv_clientversion"],
        "41080067"
    );
}

#[test]
fn backslash_paths_route_identically() {
    let data = synthetic_db("CREATE TABLE t (id INTEGER);");
    let path = "[P2]\\Users\\admin\\Documents\\xwechat_files\\wxid_zuaa9igqlro22_eef8\\db_storage\\favorite\\favorite.db";
    let (status, payload, error) = run_plugin(path, &data);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let artifact = &payload.expect("payload")["artifacts"][0];
    assert_eq!(artifact["attrs"]["category"], "favorite");
    assert_eq!(artifact["attrs"]["wxid"], "wxid_zuaa9igqlro22_eef8");
}

// ---------------------------------------------------------------------------
// Content parsing (v2): contacts / sessions / messages / moments / favorites
// ---------------------------------------------------------------------------

const OWNER_SEGMENT: &str = "wxid_owner22_ab12";

fn content_path(category: &str, name: &str) -> String {
    format!("[P2]/Users/admin/Documents/xwechat_files/{OWNER_SEGMENT}/db_storage/{category}/{name}")
}

#[test]
fn contact_db_yields_contact_artifacts() {
    let data = synthetic_db(
        "CREATE TABLE contact (id INTEGER PRIMARY KEY, username TEXT, local_type INTEGER,
            alias TEXT, encrypt_username TEXT, delete_flag INTEGER, remark TEXT,
            nick_name TEXT, head_img_md5 TEXT, description TEXT);
        INSERT INTO contact (username, local_type, alias, encrypt_username, delete_flag,
            remark, nick_name, head_img_md5, description) VALUES
            ('wxid_friend22', 0, '', '', 0, '闺蜜', '小倩', 'abc123', ''),
            ('weixin', 0, '', '', 0, '', '微信团队', '', 'system account'),
            ('wxid_gone22', 3, 'gonealias', 'v1_xyz@stranger', 1, '', '已删除', '', '');",
    );
    let (status, payload, error) = run_plugin(&content_path("contact", "contact.db"), &data);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    let contacts = artifacts_of(&payload, "WeChatContact");
    assert_eq!(contacts.len(), 3);
    // Inventory artifact is still emitted alongside the content artifacts.
    assert_eq!(artifacts_of(&payload, "WeChatDatabase").len(), 1);

    let friend = contacts
        .iter()
        .find(|a| a["attrs"]["username"] == "wxid_friend22")
        .expect("friend contact");
    assert_eq!(friend["attrs"]["nickName"], "小倩");
    assert_eq!(friend["attrs"]["remark"], "闺蜜");
    assert_eq!(friend["attrs"]["localType"], 0);
    assert_eq!(friend["attrs"]["headImgMd5"], "abc123");
    assert!(friend["attrs"].get("deleted").is_none());
    assert_eq!(friend["title"], "联系人 闺蜜");

    let gone = contacts
        .iter()
        .find(|a| a["attrs"]["username"] == "wxid_gone22")
        .expect("deleted contact");
    assert_eq!(gone["attrs"]["deleted"], true);
    // encrypt_username is passed through verbatim, never decoded.
    assert_eq!(gone["attrs"]["encryptUsername"], "v1_xyz@stranger");
    assert_eq!(gone["attrs"]["alias"], "gonealias");
}

#[test]
fn session_db_yields_session_artifacts() {
    let data = synthetic_db(
        "CREATE TABLE SessionTable (username TEXT PRIMARY KEY, type INTEGER,
            unread_count INTEGER, is_hidden INTEGER, summary TEXT,
            last_timestamp INTEGER, last_msg_sender TEXT,
            last_sender_display_name TEXT, last_msg_type INTEGER);
        INSERT INTO SessionTable VALUES
            ('wxid_friend22', 2, 3, 0, '晚上见', 1700000000, 'wxid_friend22', '小倩', 1),
            ('weixin', 0, 0, 1, '', 0, '', '', 49);",
    );
    let (status, payload, error) = run_plugin(&content_path("session", "session.db"), &data);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    let sessions = artifacts_of(&payload, "WeChatSession");
    assert_eq!(sessions.len(), 2);
    let friend = sessions
        .iter()
        .find(|a| a["attrs"]["username"] == "wxid_friend22")
        .expect("friend session");
    assert_eq!(friend["attrs"]["unreadCount"], 3);
    assert_eq!(friend["attrs"]["summary"], "晚上见");
    assert_eq!(friend["attrs"]["lastSenderDisplayName"], "小倩");
    assert_eq!(friend["attrs"]["isHidden"], false);
    assert_eq!(friend["attrs"]["lastTimestampUtc"], "2023-11-14T22:13:20Z");
    let team = sessions
        .iter()
        .find(|a| a["attrs"]["username"] == "weixin")
        .expect("weixin session");
    assert_eq!(team["attrs"]["isHidden"], true);
    // last_timestamp = 0 yields no timestamp attr.
    assert!(team["attrs"].get("lastTimestampUtc").is_none());
}

/// Name2Id rowids: 1 = owner (bare wxid, path segment carries `_ab12`),
/// 2 = friend. One Msg table per friend plus one orphan table whose suffix
/// matches nobody.
fn synthetic_message_db() -> (Vec<u8>, String) {
    use md5::{Digest, Md5};
    let friend_table = format!("Msg_{:x}", Md5::digest(b"friend_wxid44"));
    let orphan_table = format!("Msg_{:x}", Md5::digest(b"ghost_wxid99"));
    let long_text: String = "长".repeat(600);
    let zstd_blob = zstd::encode_all(&b"compressed hello"[..], 0).expect("zstd encode");
    let data = synthetic_db_with(|conn| {
        conn.execute_batch(
            "CREATE TABLE Name2Id (user_name TEXT PRIMARY KEY, is_session INTEGER);
            INSERT INTO Name2Id (rowid, user_name, is_session) VALUES
                (1, 'wxid_owner22', 1), (2, 'friend_wxid44', 1);",
        )
        .expect("name2id");
        for table in [&friend_table, &orphan_table] {
            conn.execute_batch(&format!(
                "CREATE TABLE \"{table}\" (local_id INTEGER PRIMARY KEY, server_id INTEGER,
                    local_type INTEGER, real_sender_id INTEGER, create_time INTEGER,
                    message_content TEXT, WCDB_CT_message_content INTEGER);"
            ))
            .expect("msg table");
        }
        let mut stmt = conn
            .prepare(&format!(
                "INSERT INTO \"{friend_table}\" (local_id, server_id, local_type,
                    real_sender_id, create_time, message_content, WCDB_CT_message_content)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
            ))
            .expect("insert stmt");
        // Outgoing plaintext text message.
        stmt.execute(rusqlite::params![
            1,
            100,
            1,
            1,
            1700000000i64,
            "你好",
            None::<i64>
        ])
        .expect("row 1");
        // Incoming zstd-compressed text message (WCDB_CT = 4).
        stmt.execute(rusqlite::params![
            2,
            101,
            1,
            2,
            1700000060i64,
            zstd_blob,
            Some(4i64)
        ])
        .expect("row 2");
        // Incoming picture, unknown-sender rowid (no Name2Id entry).
        stmt.execute(rusqlite::params![
            3,
            102,
            3,
            99,
            1700000120i64,
            "",
            None::<i64>
        ])
        .expect("row 3");
        // Long message must be retained in full.
        stmt.execute(rusqlite::params![
            4,
            103,
            1,
            2,
            1700000180i64,
            long_text,
            None::<i64>
        ])
        .expect("row 4");
        // Unknown local_type keeps the raw number only.
        stmt.execute(rusqlite::params![
            5,
            104,
            424242,
            2,
            1700000240i64,
            "?",
            None::<i64>
        ])
        .expect("row 5");
        drop(stmt);
        let mut orphan = conn
            .prepare(&format!(
                "INSERT INTO \"{orphan_table}\" (local_id, server_id, local_type,
                    real_sender_id, create_time, message_content, WCDB_CT_message_content)
                 VALUES (1, 200, 1, 2, 1700000300, 'orphan', NULL)"
            ))
            .expect("orphan stmt");
        orphan.execute([]).expect("orphan row");
    });
    (data, friend_table)
}

#[test]
fn message_db_yields_message_artifacts_and_timeline() {
    let (data, friend_table) = synthetic_message_db();
    let (status, payload, error) = run_plugin(&content_path("message", "message_0.db"), &data);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    let messages = artifacts_of(&payload, "WeChatMessage");
    assert_eq!(messages.len(), 6);
    let events = payload["timelineEvents"]
        .as_array()
        .expect("timelineEvents");
    assert_eq!(events.len(), 6);
    assert_eq!(payload["timesAreLocal"], false);

    // Direction: rowid 1 resolves to the bare owner wxid and matches the
    // path segment despite its `_ab12` suffix.
    let outgoing = messages
        .iter()
        .find(|a| a["attrs"]["localId"] == 1 && a["attrs"]["talkerTable"] == friend_table)
        .expect("outgoing");
    assert_eq!(outgoing["attrs"]["talker"], "friend_wxid44");
    assert_eq!(outgoing["attrs"]["senderUsername"], "wxid_owner22");
    assert_eq!(outgoing["attrs"]["isSend"], true);
    assert_eq!(outgoing["attrs"]["serverId"], 100);
    assert_eq!(outgoing["attrs"]["localTypeLabel"], "文本");
    assert_eq!(outgoing["attrs"]["createTimeUtc"], "2023-11-14T22:13:20Z");
    assert_eq!(outgoing["attrs"]["contentText"], "你好");
    assert_eq!(outgoing["attrs"]["zstdCompressed"], false);
    assert!(outgoing["attrs"].get("contentTruncated").is_none());

    // zstd round trip.
    let compressed = messages
        .iter()
        .find(|a| a["attrs"]["localId"] == 2)
        .expect("compressed");
    assert_eq!(compressed["attrs"]["zstdCompressed"], true);
    assert_eq!(compressed["attrs"]["contentText"], "compressed hello");
    assert_eq!(compressed["attrs"]["senderUsername"], "friend_wxid44");
    assert_eq!(compressed["attrs"]["isSend"], false);

    // Unknown sender rowid: no isSend attr.
    let unknown_sender = messages
        .iter()
        .find(|a| a["attrs"]["localId"] == 3)
        .expect("unknown sender");
    assert!(unknown_sender["attrs"].get("isSend").is_none());
    assert_eq!(unknown_sender["attrs"]["localTypeLabel"], "图片");

    // Full message body is preserved; only the human summary is shortened.
    let long = messages
        .iter()
        .find(|a| a["attrs"]["localId"] == 4)
        .expect("long message");
    assert!(long["attrs"].get("contentTruncated").is_none());
    assert_eq!(
        long["attrs"]["contentText"]
            .as_str()
            .expect("text")
            .chars()
            .count(),
        600
    );

    // Unknown local_type: raw number only.
    let odd = messages
        .iter()
        .find(|a| a["attrs"]["localId"] == 5)
        .expect("odd type");
    assert_eq!(odd["attrs"]["localType"], 424242);
    assert!(odd["attrs"].get("localTypeLabel").is_none());

    // Orphan Msg_ table: md5 reverse lookup misses → no talker attr.
    let orphan = messages
        .iter()
        .find(|a| {
            a["attrs"]["talkerTable"]
                .as_str()
                .unwrap_or_default()
                .contains("ghost")
                || a["attrs"].get("talker").is_none()
        })
        .expect("orphan message");
    assert!(orphan["attrs"].get("talker").is_none());

    // Timeline events carry RFC3339 UTC timestamps (table order follows the
    // sqlite_master name ordering, so match on the set).
    let timestamps: Vec<&str> = events
        .iter()
        .filter_map(|e| e["timestampUtc"].as_str())
        .collect();
    assert!(timestamps.contains(&"2023-11-14T22:13:20Z"));
    assert!(events.iter().all(|e| e["eventType"] == "WeChatMessage"));

    assert!(warning_texts(&payload).is_empty());
}

#[test]
fn message_db_with_bad_zstd_row_warns_but_continues() {
    use md5::{Digest, Md5};
    let friend_table = format!("Msg_{:x}", Md5::digest(b"friend_wxid44"));
    let data = synthetic_db_with(|conn| {
        conn.execute_batch(
            "CREATE TABLE Name2Id (user_name TEXT PRIMARY KEY, is_session INTEGER);
            INSERT INTO Name2Id (rowid, user_name, is_session) VALUES (2, 'friend_wxid44', 1);",
        )
        .expect("name2id");
        conn.execute_batch(&format!(
            "CREATE TABLE \"{friend_table}\" (local_id INTEGER PRIMARY KEY, server_id INTEGER,
                local_type INTEGER, real_sender_id INTEGER, create_time INTEGER,
                message_content TEXT, WCDB_CT_message_content INTEGER);"
        ))
        .expect("msg table");
        conn.execute(
            &format!(
                "INSERT INTO \"{friend_table}\" VALUES (1, 1, 1, 2, 1700000000, ?1, 4), (2, 2, 1, 2, 1700000060, 'plain', 0)"
            ),
            [b"not-really-zstd".as_slice()],
        )
        .expect("rows");
    });
    let (status, payload, error) = run_plugin(&content_path("message", "message_0.db"), &data);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    let messages = artifacts_of(&payload, "WeChatMessage");
    assert_eq!(messages.len(), 2);
    let bad = messages
        .iter()
        .find(|a| a["attrs"]["localId"] == 1)
        .expect("bad zstd row");
    assert_eq!(bad["attrs"]["zstdCompressed"], true);
    assert!(bad["attrs"].get("contentText").is_none());
    assert!(warning_texts(&payload)
        .iter()
        .any(|w| w.contains("无法解压")));
}

#[test]
fn biz_message_recovers_compressed_body_and_official_account_reply_xml() {
    use md5::{Digest, Md5};
    let talker = "gh_case_owner";
    let table = format!("Msg_{:x}", Md5::digest(talker.as_bytes()));
    let body_xml =
        "<msg><appmsg><type>5</type><title>号主答复标题</title><des>答复摘要</des></appmsg></msg>";
    let body = zstd::encode_all(body_xml.as_bytes(), 0).expect("compress body");
    let source = "<msg><sourceusername>gh_case_owner</sourceusername><sourcedisplayname>案件号主</sourcedisplayname><replyusername>wxid_reader</replyusername><replynickname>提问者</replynickname><content>号主回复正文</content></msg>";
    let mut packed = vec![1, 2];
    packed.extend_from_slice("<extra><content>号主补充回复</content></extra>".as_bytes());
    let data = synthetic_db_with(|conn| {
        conn.execute_batch(&format!(
            "CREATE TABLE Name2Id (user_name TEXT PRIMARY KEY, is_session INTEGER);
             INSERT INTO Name2Id (rowid, user_name, is_session) VALUES (2, '{talker}', 1);
             CREATE TABLE \"{table}\" (
                local_id INTEGER PRIMARY KEY, server_id INTEGER, local_type INTEGER,
                real_sender_id INTEGER, create_time INTEGER, source TEXT,
                message_content TEXT, compress_content BLOB, packed_info_data BLOB,
                WCDB_CT_message_content INTEGER, WCDB_CT_source INTEGER);"
        ))
        .expect("biz schema");
        conn.execute(
            &format!(
                "INSERT INTO \"{table}\" VALUES (1, 501, 49, 0, 1700000000, ?1, '', ?2, ?3, 0, 0)"
            ),
            rusqlite::params![source, body, packed],
        )
        .expect("biz row");
    });

    let (status, payload, error) = run_plugin(&content_path("message", "biz_message_0.db"), &data);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let messages = artifacts_of(&payload.expect("payload"), "WeChatMessage");
    let attrs = &messages[0]["attrs"];
    assert_eq!(attrs["appTitle"], "号主答复标题");
    assert_eq!(attrs["xmlText"], "号主答复标题\n答复摘要");
    assert_eq!(attrs["sourceUsername"], "gh_case_owner");
    assert_eq!(attrs["sourceDisplayName"], "案件号主");
    assert_eq!(attrs["senderUsername"], "gh_case_owner");
    assert_eq!(attrs["replyUsername"], "wxid_reader");
    assert_eq!(attrs["replyNickname"], "提问者");
    assert_eq!(attrs["sourceXmlText"], "号主回复正文");
    assert!(attrs["packedInfoText"]
        .as_str()
        .is_some_and(|text| text.contains("号主补充回复")));
}

#[test]
fn sns_db_yields_moment_artifacts() {
    let xml_with_media =
        "<SnsDataItem><TimelineObject><id>111</id><username>wxid_friend22</username>\
        <createTime>1700000000</createTime><contentDesc>周末爬山</contentDesc>\
        <mediaList><media id=\"1\"><url>https://media.invalid/1</url></media></mediaList></TimelineObject></SnsDataItem>";
    let xml_plain = "<SnsDataItem><TimelineObject><id>222</id><username>wxid_friend22</username>\
        <createTime>bad</createTime><contentDesc></contentDesc><mediaList/></TimelineObject></SnsDataItem>";
    let data = synthetic_db_with(|conn| {
        conn.execute_batch(
            "CREATE TABLE SnsTimeLine (tid INTEGER PRIMARY KEY, user_name TEXT, content TEXT, pack_info_buf TEXT);",
        )
        .expect("sns table");
        conn.execute(
            "INSERT INTO SnsTimeLine (tid, user_name, content, pack_info_buf) VALUES
             (1, 'wxid_friend22', ?1, '<likeUser><username>wxid_like</username><nickname>点赞者</nickname></likeUser><commentUser><username>wxid_comment</username><content>评论正文</content></commentUser>'),
             (2, 'wxid_friend22', ?2, '')",
            [xml_with_media, xml_plain],
        )
        .expect("sns rows");
    });
    let (status, payload, error) = run_plugin(&content_path("sns", "sns.db"), &data);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    let moments = artifacts_of(&payload, "WeChatMoment");
    assert_eq!(moments.len(), 2);
    let with_media = moments
        .iter()
        .find(|a| a["attrs"]["tid"] == 1)
        .expect("media moment");
    assert_eq!(with_media["attrs"]["contentDesc"], "周末爬山");
    assert_eq!(with_media["attrs"]["hasMedia"], true);
    assert_eq!(with_media["attrs"]["mediaCount"], 1);
    assert_eq!(
        with_media["attrs"]["mediaItems"][0]["url"],
        "https://media.invalid/1"
    );
    assert_eq!(with_media["attrs"]["likeCount"], 1);
    assert_eq!(with_media["attrs"]["likes"][0]["nickname"], "点赞者");
    assert_eq!(with_media["attrs"]["commentCount"], 1);
    assert_eq!(with_media["attrs"]["comments"][0]["content"], "评论正文");
    assert_eq!(with_media["attrs"]["snsId"], "111");
    assert_eq!(with_media["attrs"]["createTimeUtc"], "2023-11-14T22:13:20Z");
    let plain = moments
        .iter()
        .find(|a| a["attrs"]["tid"] == 2)
        .expect("plain moment");
    assert_eq!(plain["attrs"]["hasMedia"], false);
    // Unparseable createTime: no timestamp attr, no timeline event.
    assert!(plain["attrs"].get("createTimeUtc").is_none());
    let events = payload["timelineEvents"]
        .as_array()
        .expect("timelineEvents");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["eventType"], "WeChatMoment");
}

#[test]
fn message_resource_db_projects_inline_media_and_blob_hash() {
    let data = synthetic_db_with(|conn| {
        conn.execute_batch(
            "CREATE TABLE MediaResource (local_id INTEGER, kind TEXT, payload BLOB);",
        )
        .expect("resource table");
        conn.execute(
            "INSERT INTO MediaResource (local_id, kind, payload) VALUES (1, 'thumb', ?1)",
            [b"\x89PNG\r\n\x1a\nbody".as_slice()],
        )
        .expect("resource row");
    });
    let (status, payload, error) =
        run_plugin(&content_path("message", "message_resource.db"), &data);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    let media = artifacts_of(&payload, "WeChatMedia");
    assert_eq!(media.len(), 1);
    let blob = &media[0]["attrs"]["values"]["payload"];
    assert_eq!(blob["mimeType"], "image/png");
    assert_eq!(blob["sha256"].as_str().map(str::len), Some(64));
    assert!(blob["inlineDataBase64"].as_str().is_some());
}

#[test]
fn favorite_db_yields_favorite_artifacts() {
    let data = synthetic_db(
        "CREATE TABLE fav_db_item (local_id INTEGER PRIMARY KEY, server_id INTEGER,
            type INTEGER, update_time INTEGER, fromusr TEXT, realchatname TEXT, content TEXT);
        INSERT INTO fav_db_item VALUES (7, 900, 2, 1700000000, 'wxid_friend22', '闺蜜群', '收藏的链接内容');",
    );
    let (status, payload, error) = run_plugin(&content_path("favorite", "favorite.db"), &data);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    let favorites = artifacts_of(&payload, "WeChatFavorite");
    assert_eq!(favorites.len(), 1);
    let attrs = &favorites[0]["attrs"];
    assert_eq!(attrs["localId"], 7);
    assert_eq!(attrs["serverId"], 900);
    assert_eq!(attrs["type"], 2);
    assert_eq!(attrs["fromUsr"], "wxid_friend22");
    assert_eq!(attrs["realChatName"], "闺蜜群");
    assert_eq!(attrs["updateTimeUtc"], "2023-11-14T22:13:20Z");
    assert_eq!(attrs["contentText"], "收藏的链接内容");
}

#[test]
fn empty_favorite_db_is_ok_and_silent() {
    let data = synthetic_db(
        "CREATE TABLE fav_db_item (local_id INTEGER PRIMARY KEY, server_id INTEGER,
            type INTEGER, update_time INTEGER, fromusr TEXT, realchatname TEXT, content TEXT);",
    );
    let (status, payload, error) = run_plugin(&content_path("favorite", "favorite.db"), &data);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    assert!(artifacts_of(&payload, "WeChatFavorite").is_empty());
    assert_eq!(artifacts_of(&payload, "WeChatDatabase").len(), 1);
    assert!(warning_texts(&payload).is_empty());
}

#[test]
fn artifact_cap_truncates_with_warning() {
    let data = synthetic_db_with(|conn| {
        conn.execute_batch(
            "CREATE TABLE contact (id INTEGER PRIMARY KEY, username TEXT, local_type INTEGER,
                alias TEXT, encrypt_username TEXT, delete_flag INTEGER, remark TEXT,
                nick_name TEXT, head_img_md5 TEXT, description TEXT);",
        )
        .expect("contact table");
        let tx = conn.unchecked_transaction().expect("tx");
        {
            let mut stmt = tx
                .prepare("INSERT INTO contact (username) VALUES (?1)")
                .expect("insert");
            for index in 0..20_001 {
                stmt.execute([format!("wxid_{index}")]).expect("row");
            }
        }
        tx.commit().expect("commit");
    });
    let (status, payload, error) = run_plugin(&content_path("contact", "contact.db"), &data);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    assert_eq!(artifacts_of(&payload, "WeChatContact").len(), 20_000);
    assert!(warning_texts(&payload)
        .iter()
        .any(|w| w.contains("20000") || w.contains("20_000") || w.contains("上限")));
}

// ---------------------------------------------------------------------------
// MEOW_WECHAT_KEYS development key-injection channel
// ---------------------------------------------------------------------------

/// Build a synthetic WCDB/SQLCipher-4 encrypted blob whose pages carry a
/// valid HMAC for `key` (mirrors `sqlcipher4`'s layout) but whose plaintext
/// is not a SQLite database — enough to drive the decrypt-then-fallback
/// path end to end.
fn synthetic_encrypted_blob(key: &[u8; 32], pages: u32) -> Vec<u8> {
    use aes::cipher::{BlockEncryptMut, KeyIvInit};
    use hmac::{Hmac, Mac};
    use sha2::Sha512;
    type Enc = cbc::Encryptor<aes::Aes256>;

    let salt = [0x11u8; 16];
    let iv = [0x22u8; 16];
    let mac_salt: Vec<u8> = salt.iter().map(|b| b ^ 0x3a).collect();
    let mac_key = pbkdf2::pbkdf2_hmac_array::<Sha512, 32>(key, &mac_salt, 2);
    let mut out = Vec::new();
    for page_no in 1..=pages {
        // Page 1 carries a 16-byte salt prefix, so its ciphertext body is
        // 16 bytes shorter than later pages'.
        let body_len = if page_no == 1 { 4000 } else { 4016 };
        let mut body = vec![0u8; body_len];
        for (i, b) in body.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        // NoPadding on a block-aligned buffer encrypts in place.
        Enc::new((&key[..]).into(), (&iv[..]).into())
            .encrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut body, body_len)
            .expect("encrypt");
        let ciphertext = body;
        let mut mac = <Hmac<Sha512> as Mac>::new_from_slice(&mac_key[..]).expect("hmac");
        mac.update(&ciphertext);
        mac.update(&iv);
        mac.update(&page_no.to_le_bytes());
        let tag = mac.finalize().into_bytes();
        if page_no == 1 {
            out.extend_from_slice(&salt);
        }
        out.extend_from_slice(&ciphertext);
        out.extend_from_slice(&iv);
        out.extend_from_slice(&tag[..64]);
    }
    out
}

/// Single test for every env-var-dependent branch so the process-wide
/// environment is never mutated concurrently by parallel tests.
#[test]
fn key_injection_channel_branches() {
    use std::io::Write;
    let key = [0x42u8; 32];
    let key_hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
    let blob = synthetic_encrypted_blob(&key, 3);
    let path = content_path("message", "message_0.db");

    // 1. Channel inactive: pure v1 inventory behavior.
    std::env::remove_var("MEOW_WECHAT_KEYS");
    let (status, payload, error) = run_plugin(&path, &blob);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    assert_eq!(artifacts_of(&payload, "WeChatDatabase").len(), 1);
    assert!(artifacts_of(&payload, "WeChatMessage").is_empty());
    assert!(warning_texts(&payload)
        .iter()
        .any(|w| w.contains("WCDB/SQLCipher")));

    // 2. Channel active, key file has no entry for this db: warning +
    // inventory fallback.
    let dir = std::env::temp_dir().join(format!("wechat-keys-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let keys_file = dir.join("keys.json");
    std::fs::write(&keys_file, br#"{"other.db": "00"}"#).expect("write keys");
    std::env::set_var("MEOW_WECHAT_KEYS", &keys_file);
    let (status, payload, error) = run_plugin(&path, &blob);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    assert_eq!(
        artifacts_of(&payload, "WeChatDatabase")[0]["attrs"]["encrypted"],
        true
    );
    assert!(warning_texts(&payload)
        .iter()
        .any(|w| w.contains("密钥注入通道未能解密")));

    // 3. Key matches (decrypt + page-1 HMAC pass) but the plaintext is not
    // SQLite: warn and fall back to inventory, never ParseError, and the
    // key material never crosses into the payload.
    let mut file = std::fs::File::create(&keys_file).expect("rewrite keys");
    write!(file, "{{\"ds:1:wechat-1\": \"{key_hex}\"}}").expect("write key");
    drop(file);
    let (status, payload, error) = run_plugin(&path, &blob);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    let warnings = warning_texts(&payload);
    assert!(warnings
        .iter()
        .any(|w| w.contains("解密产物不是可读的 SQLite 库")));
    assert_eq!(artifacts_of(&payload, "WeChatDatabase").len(), 1);
    assert!(!payload.to_string().contains(&key_hex));

    // 3b. Existing logical-path key files remain compatible after file ids
    // became the preferred cross-source identity.
    let mut file = std::fs::File::create(&keys_file).expect("rewrite path key");
    write!(file, "{{\"{path}\": \"{key_hex}\"}}").expect("write path key");
    drop(file);
    let (status, payload, error) = run_plugin(&path, &blob);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    assert!(warning_texts(&payload.expect("payload"))
        .iter()
        .any(|w| w.contains("解密产物不是可读的 SQLite 库")));

    // 3c. Single-account key files using only the basename also stay
    // compatible with the offline tooling format.
    let mut file = std::fs::File::create(&keys_file).expect("rewrite basename key");
    write!(file, "{{\"message_0.db\": \"{key_hex}\"}}").expect("write basename key");
    drop(file);
    let (status, payload, error) = run_plugin(&path, &blob);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    assert!(warning_texts(&payload.expect("payload"))
        .iter()
        .any(|w| w.contains("解密产物不是可读的 SQLite 库")));

    // 4. Key file unreadable: warning + fallback, still no ParseError.
    std::env::set_var("MEOW_WECHAT_KEYS", dir.join("missing.json"));
    let (status, payload, error) = run_plugin(&path, &blob);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    assert!(warning_texts(&payload.expect("payload"))
        .iter()
        .any(|w| w.contains("密钥文件不可读")));

    std::env::remove_var("MEOW_WECHAT_KEYS");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn decrypted_real_layout_parses_after_wal_downgrade() {
    // Decrypted WCDB databases carry WAL read/write versions in the header;
    // db.rs downgrades them on the private copy so deserialize works.
    let mut data = synthetic_db("CREATE TABLE t (id INTEGER);");
    assert_eq!(data[18], 1);
    data[18] = 2;
    data[19] = 2;
    let (status, payload, error) = run_plugin(&content_path("sns", "sns.db"), &data);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    assert_eq!(artifacts_of(&payload, "WeChatDatabase").len(), 1);
    assert_eq!(payload["artifacts"][0]["attrs"]["tableCount"], 1);
}

// ---- Optional action channel (`meow_plugin_action`) ----

fn run_action(request: &Value) -> (MeowStatus, Option<Value>, Option<String>) {
    let body = request.to_string();
    let response = unsafe { meow_plugin_action(body.as_ptr(), body.len() as u64) };
    let status = response.status;
    let (payload, error) = unsafe { drain_response(&response) };
    let payload = payload.map(|bytes| serde_json::from_slice(&bytes).expect("valid JSON"));
    (status, payload, error)
}

/// Build a synthetic WCDB-encrypted page 1 whose HMAC validates for `key`
/// (same construction as sqlcipher4: mac key = PBKDF2-HMAC-SHA512(key,
/// salt ^ 0x3a, 2), HMAC over ciphertext | IV | pgno_le).
fn synthetic_encrypted_page1(key: &[u8; 32], salt: &[u8; 16]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha2::Sha512;
    let mut page = vec![0u8; sqlcipher4::PAGE_SZ - sqlcipher4::SALT_SZ];
    for (index, byte) in page.iter_mut().enumerate() {
        *byte = (index % 251) as u8;
    }
    let body_end = page.len() - 80 + 16; // ciphertext + IV
    let mac_salt: Vec<u8> = salt.iter().map(|b| b ^ 0x3a).collect();
    let mac_key = pbkdf2::pbkdf2_hmac_array::<Sha512, 32>(key, &mac_salt, 2);
    let mut mac = <Hmac<Sha512> as Mac>::new_from_slice(&mac_key[..]).expect("hmac key");
    mac.update(&page[..body_end]);
    mac.update(&1u32.to_le_bytes());
    let tag = mac.finalize().into_bytes();
    page[body_end..body_end + 64].copy_from_slice(&tag[..64]);
    let mut full = salt.to_vec();
    full.extend_from_slice(&page);
    assert!(sqlcipher4::validate_page1(key, &full), "fixture page1");
    full
}

fn write_temp_dump(key: &[u8; 32]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("meow-wechat-action-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dump dir");
    let dump = dir.join("dump.dmp");
    let mut bytes = vec![0u8; 1024];
    bytes.extend_from_slice(format!("x'{}'", keyscan::key_to_hex(key)).as_bytes());
    bytes.extend_from_slice(&[7u8; 512]);
    std::fs::write(&dump, &bytes).expect("write dump");
    dump
}

#[test]
fn action_describe_declares_recover_keys() {
    let (status, payload, error) = run_action(&serde_json::json!({"action": "describe"}));
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    let actions = payload["actions"].as_array().expect("actions");
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0]["id"], "recoverKeys");
    assert_eq!(actions[0]["label"], "从内存镜像恢复数据库密钥");
    assert_eq!(actions[0]["inputKind"], "file");
}

#[test]
fn action_unknown_is_unsupported() {
    let (status, _payload, error) = run_action(&serde_json::json!({"action": "nope"}));
    assert_eq!(status, MeowStatus::Unsupported);
    assert!(error.is_some());
}

#[test]
fn action_malformed_json_is_parse_error() {
    let body = b"not json";
    let response = unsafe { meow_plugin_action(body.as_ptr(), body.len() as u64) };
    assert_eq!(response.status, MeowStatus::ParseError);
    let (_payload, error) = unsafe { drain_response(&response) };
    assert!(error.is_some());
}

#[test]
fn action_recover_keys_requires_dump_path() {
    let (status, _payload, error) =
        run_action(&serde_json::json!({"action": "recoverKeys", "params": {}}));
    assert_eq!(status, MeowStatus::ParseError);
    assert!(error.is_some());
}

#[test]
fn action_recover_keys_matches_validated_page1() {
    use base64::Engine as _;
    let key = [0x42u8; 32];
    let salt = [0x11u8; 16];
    let page1 = synthetic_encrypted_page1(&key, &salt);
    let dump = write_temp_dump(&key);
    let request = serde_json::json!({
        "action": "recoverKeys",
        "params": {
            "dumpPath": dump.to_string_lossy(),
            "dbPages": {
                "message_0.db": base64::engine::general_purpose::STANDARD.encode(&page1),
                "contact.db": base64::engine::general_purpose::STANDARD.encode(vec![9u8; 4096]),
                "broken.db": "!!!not-base64!!!",
            }
        }
    });
    let (status, payload, error) = run_action(&request);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    assert_eq!(payload["candidatesSeen"], 1);
    assert_eq!(payload["matched"], serde_json::json!(["message_0.db"]));
    let unmatched = payload["unmatched"].as_array().expect("unmatched");
    assert!(unmatched.contains(&serde_json::json!("contact.db")));
    assert!(unmatched.contains(&serde_json::json!("broken.db")));
    assert_eq!(
        payload["keys"]["message_0.db"],
        serde_json::Value::String(keyscan::key_to_hex(&key))
    );
    let _ = std::fs::remove_dir_all(dump.parent().expect("dump dir"));
}
