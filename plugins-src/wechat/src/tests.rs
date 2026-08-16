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

fn request_for<'a>(path: &'a CString, id: &'a CString, data: &'a [u8]) -> MeowExtractRequest {
    MeowExtractRequest {
        struct_size: std::mem::size_of::<MeowExtractRequest>() as u32,
        file_path: path.as_ptr().cast(),
        file_id: id.as_ptr().cast(),
        data: data.as_ptr(),
        data_len: data.len() as u64,
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
        assert_eq!(CStr::from_ptr(info.plugin_version.cast()), c"0.1.0");
        assert_eq!(CStr::from_ptr(info.display_name.cast()), c"微信");
        let families: Value = serde_json::from_str(
            CStr::from_ptr(info.families_json.cast())
                .to_str()
                .expect("families utf8"),
        )
        .expect("families json");
        assert_eq!(
            families,
            serde_json::json!(["WeChatInstall", "WeChatAccount", "WeChatDatabase"])
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
                "config.ini"
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
