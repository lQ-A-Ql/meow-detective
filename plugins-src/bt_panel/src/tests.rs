//! Plugin-internal unit tests: synthetic SQLite databases built with
//! rusqlite cover every family, the path self-filter, corrupt-input
//! ParseError, and panic self-capture.

use super::*;
use rusqlite::Connection;
use serde_json::Value;
use std::ffi::CString;

const PANEL_PREFIX: &str = "[P2]/www/server/panel/data/";

/// Build a synthetic panel database: run `schema`, then serialize to bytes.
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

fn cstrings(basename: &str) -> (CString, CString) {
    let path = CString::new(format!("{PANEL_PREFIX}{basename}")).expect("path");
    let id = CString::new("ds:1:bt-1").expect("id");
    (path, id)
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

fn run_plugin(basename: &str, data: &[u8]) -> (MeowStatus, Option<Value>, Option<String>) {
    let (path, id) = cstrings(basename);
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
    assert_eq!(info.evidence_platform, MeowEvidencePlatform::Linux);
    unsafe {
        assert_eq!(
            CStr::from_ptr(info.plugin_id.cast()),
            c"meow.plugin.bt_panel"
        );
        assert_eq!(CStr::from_ptr(info.plugin_version.cast()), c"0.1.0");
        assert_eq!(
            CStr::from_ptr(info.path_patterns_json.cast()),
            c"[\"*.db\"]"
        );
        let families: Value = serde_json::from_str(
            CStr::from_ptr(info.families_json.cast())
                .to_str()
                .expect("families utf8"),
        )
        .expect("families json");
        assert_eq!(
            families,
            serde_json::json!([
                "BtPanelAccount",
                "BtPanelSite",
                "BtPanelDatabase",
                "BtPanelFtp",
                "BtPanelFirewall",
                "BtPanelTask",
                "BtPanelLog"
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
fn non_panel_path_is_silent_ok() {
    let path = CString::new("[P0]/var/lib/mysql/mysql/user.db").expect("path");
    let id = CString::new("ds:1:x").expect("id");
    let data = b"not even sqlite";
    let request = request_for(&path, &id, data);
    let response = unsafe { meow_plugin_extract(&request) };
    assert_eq!(response.status, MeowStatus::Ok);
    let (payload, error) = unsafe { drain_response(&response) };
    assert!(error.is_none());
    let payload: Value = serde_json::from_slice(&payload.expect("payload")).expect("valid JSON");
    assert_eq!(payload["artifacts"].as_array().expect("artifacts").len(), 0);
}

#[test]
fn unknown_panel_db_is_silent_ok() {
    // docker.db / panel.db / task.db live in the same directory but are out
    // of scope for this plugin version.
    let data = synthetic_db("CREATE TABLE whatever (id INTEGER);");
    let (status, payload, error) = run_plugin("db/docker.db", &data);
    assert_eq!(status, MeowStatus::Ok);
    assert!(error.is_none());
    let payload = payload.expect("payload");
    assert_eq!(payload["artifacts"].as_array().expect("artifacts").len(), 0);
}

#[test]
fn corrupt_database_is_parse_error() {
    let mut data = b"SQLite format 3\0".to_vec();
    data.extend_from_slice(&[0xFFu8; 256]);
    let (status, _, error) = run_plugin("default.db", &data);
    assert_eq!(status, MeowStatus::ParseError);
    assert!(error.is_some());
}

#[test]
fn accounts_are_parsed_and_redacted() {
    let data = synthetic_db(
        "CREATE TABLE users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT, password TEXT, salt TEXT,
            login_ip TEXT, login_time TEXT, phone TEXT, email TEXT
        );
        INSERT INTO users (username, password, salt, login_ip, login_time, email)
        VALUES ('admin', 'b59c67bf196a4758191e42f76670ceba', 's3cr3t',
                '10.0.0.8', '2025-06-01 08:00:00', 'admin@example.com');",
    );
    let (status, payload, _) = run_plugin("default.db", &data);
    assert_eq!(status, MeowStatus::Ok);
    let payload = payload.expect("payload");
    let artifact = &payload["artifacts"][0];
    assert_eq!(artifact["family"], "BtPanelAccount");
    assert_eq!(artifact["title"], "admin");
    assert_eq!(artifact["attrs"]["username"], "admin");
    assert_eq!(artifact["attrs"]["userId"], 1);
    assert_eq!(artifact["attrs"]["hasPasswordHash"], true);
    assert_eq!(
        artifact["attrs"]["passwordAlgorithm"],
        "md5(md5(md5(password)+'_bt.cn')+salt)"
    );
    assert_eq!(artifact["attrs"]["loginIp"], "10.0.0.8");
    assert_eq!(artifact["attrs"]["loginTimeLocal"], "2025-06-01T08:00:00");
    // Redaction: neither the hash nor the salt may appear anywhere.
    let text = payload.to_string();
    assert!(!text.contains("b59c67bf196a4758191e42f76670ceba"));
    assert!(!text.contains("s3cr3t"));
    assert!(payload["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .any(|w| w.as_str().unwrap_or_default().contains("wall clock")));
}

#[test]
fn sites_join_domains_and_report_orphans() {
    let data = synthetic_db(
        "CREATE TABLE sites (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT, path TEXT, status TEXT, \"index\" TEXT, ps TEXT, addtime TEXT
        );
        INSERT INTO sites (name, path, status, ps, addtime)
        VALUES ('shop.example.com', '/www/wwwroot/shop', '1', 'main shop',
                '2025-05-20 10:00:00');
        CREATE TABLE domain (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            pid INTEGER, name TEXT, port INTEGER, addtime TEXT
        );
        INSERT INTO domain (pid, name, port, addtime)
        VALUES (1, 'shop.example.com', 80, '2025-05-20 10:00:00'),
               (1, 'www.shop.example.com', 443, '2025-05-20 10:01:00'),
               (99, 'orphan.example.com', 8080, '2025-05-21 09:00:00');",
    );
    let (status, payload, _) = run_plugin("db/site.db", &data);
    assert_eq!(status, MeowStatus::Ok);
    let payload = payload.expect("payload");
    let artifacts = payload["artifacts"].as_array().expect("artifacts");
    assert_eq!(artifacts.len(), 2);
    let site = &artifacts[0];
    assert_eq!(site["family"], "BtPanelSite");
    assert_eq!(site["title"], "shop.example.com");
    assert_eq!(site["attrs"]["path"], "/www/wwwroot/shop");
    assert_eq!(site["attrs"]["statusText"], "running");
    assert_eq!(
        site["attrs"]["domains"],
        serde_json::json!(["shop.example.com:80", "www.shop.example.com:443"])
    );
    let orphan = &artifacts[1];
    assert_eq!(orphan["title"], "orphan.example.com");
    assert_eq!(orphan["attrs"]["orphan"], true);
    assert_eq!(orphan["attrs"]["port"], 8080);
}

#[test]
fn databases_ftps_firewall_crontab_families() {
    let data = synthetic_db(
        "CREATE TABLE databases (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            pid INTEGER, name TEXT, username TEXT, password TEXT,
            accept TEXT, ps TEXT, addtime TEXT
        );
        INSERT INTO databases (name, username, password, accept, addtime)
        VALUES ('shopdb', 'shopuser', 'super-secret-pw', '127.0.0.1',
                '2025-05-20 10:05:00');",
    );
    let (status, payload, _) = run_plugin("db/database.db", &data);
    assert_eq!(status, MeowStatus::Ok);
    let payload = payload.expect("payload");
    let artifact = &payload["artifacts"][0];
    assert_eq!(artifact["family"], "BtPanelDatabase");
    assert_eq!(artifact["attrs"]["databaseName"], "shopdb");
    assert_eq!(artifact["attrs"]["username"], "shopuser");
    assert_eq!(artifact["attrs"]["hasPassword"], true);
    assert!(!payload.to_string().contains("super-secret-pw"));

    let data = synthetic_db(
        "CREATE TABLE ftps (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            pid INTEGER, name TEXT, password TEXT, path TEXT,
            status TEXT, ps TEXT, addtime TEXT
        );
        INSERT INTO ftps (name, password, path, status)
        VALUES ('deploy', 'ftp-secret', '/www/wwwroot/shop', '1');",
    );
    let (status, payload, _) = run_plugin("db/ftp.db", &data);
    assert_eq!(status, MeowStatus::Ok);
    let payload = payload.expect("payload");
    let artifact = &payload["artifacts"][0];
    assert_eq!(artifact["family"], "BtPanelFtp");
    assert_eq!(artifact["attrs"]["path"], "/www/wwwroot/shop");
    assert_eq!(artifact["attrs"]["hasPassword"], true);
    assert!(!payload.to_string().contains("ftp-secret"));

    let data = synthetic_db(
        "CREATE TABLE firewall (
            id INTEGER PRIMARY KEY AUTOINCREMENT, port TEXT, ps TEXT, addtime TEXT
        );
        INSERT INTO firewall (port, ps) VALUES ('8888', 'WEB面板');
        CREATE TABLE firewall_acceptip (
            id INTEGER PRIMARY KEY AUTOINCREMENT, address TEXT, types TEXT, ps TEXT, addtime TEXT
        );
        INSERT INTO firewall_acceptip (address, types) VALUES ('1.2.3.4', 'drop');",
    );
    let (status, payload, _) = run_plugin("db/firewall.db", &data);
    assert_eq!(status, MeowStatus::Ok);
    let payload = payload.expect("payload");
    let artifacts = payload["artifacts"].as_array().expect("artifacts");
    assert_eq!(artifacts.len(), 2);
    assert_eq!(artifacts[0]["attrs"]["port"], "8888");
    assert_eq!(artifacts[0]["attrs"]["policy"], "accept");
    assert_eq!(artifacts[1]["attrs"]["sourceIp"], "1.2.3.4");
    assert_eq!(artifacts[1]["attrs"]["policy"], "drop");

    let data = synthetic_db(
        "CREATE TABLE crontab (
            id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, type TEXT,
            where1 TEXT, where_hour INTEGER, where_minute INTEGER,
            echo TEXT, sBody TEXT, addtime TEXT
        );
        INSERT INTO crontab (name, type, where1, where_hour, where_minute, echo, sBody, addtime)
        VALUES ('backup shopdb', 'day', '', 3, 30, 'backup_db.sh',
                'mysqldump shopdb > /www/backup/shop.sql', '2025-05-21 00:00:00');",
    );
    let (status, payload, _) = run_plugin("db/crontab.db", &data);
    assert_eq!(status, MeowStatus::Ok);
    let payload = payload.expect("payload");
    let artifact = &payload["artifacts"][0];
    assert_eq!(artifact["family"], "BtPanelTask");
    assert_eq!(artifact["title"], "backup shopdb");
    assert_eq!(artifact["attrs"]["cycleType"], "day");
    assert_eq!(artifact["attrs"]["hour"], 3);
    assert_eq!(artifact["attrs"]["minute"], 30);
    assert_eq!(
        artifact["attrs"]["command"],
        "mysqldump shopdb > /www/backup/shop.sql"
    );
}

#[test]
fn logs_emit_artifacts_and_timeline_events() {
    let data = synthetic_db(
        "CREATE TABLE logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT, type TEXT, log TEXT, addtime TEXT
        );
        INSERT INTO logs (type, log, addtime)
        VALUES ('用户登录', '用户[admin]登录成功', '2025-06-01 08:00:01'),
               ('网站管理', '添加站点[shop.example.com]', '2025-05-20 10:00:05'),
               ('broken', 'no timestamp row', 'not-a-time');",
    );
    let (status, payload, _) = run_plugin("db/log.db", &data);
    assert_eq!(status, MeowStatus::Ok);
    let payload = payload.expect("payload");
    assert_eq!(payload["artifacts"].as_array().expect("artifacts").len(), 3);
    let artifact = &payload["artifacts"][0];
    assert_eq!(artifact["family"], "BtPanelLog");
    assert_eq!(artifact["attrs"]["logType"], "用户登录");
    assert_eq!(artifact["attrs"]["addtimeLocal"], "2025-06-01T08:00:01");
    let events = payload["timelineEvents"].as_array().expect("events");
    // The unparsable timestamp row keeps its artifact but drops its event.
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["eventType"], "BT_PANEL_OPERATION");
    assert_eq!(events[0]["timestampUtc"], "2025-06-01T08:00:01");
}

#[test]
fn legacy_default_db_holds_all_families() {
    let data = synthetic_db(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, username TEXT, password TEXT);
        INSERT INTO users (username, password) VALUES ('admin', '21232f297a57a5a743894a0e4a801fc3');
        CREATE TABLE sites (id INTEGER PRIMARY KEY, name TEXT, path TEXT, status TEXT, addtime TEXT);
        INSERT INTO sites (name, path, status) VALUES ('legacy.example.com', '/www/wwwroot/legacy', '0');
        CREATE TABLE logs (id INTEGER PRIMARY KEY, type TEXT, log TEXT, addtime TEXT);
        INSERT INTO logs (type, log, addtime) VALUES ('面板设置', '修改面板端口', '2024-12-31 23:59:59');",
    );
    let (status, payload, _) = run_plugin("default.db", &data);
    assert_eq!(status, MeowStatus::Ok);
    let payload = payload.expect("payload");
    let families: Vec<&str> = payload["artifacts"]
        .as_array()
        .expect("artifacts")
        .iter()
        .map(|a| a["family"].as_str().expect("family"))
        .collect();
    assert!(families.contains(&"BtPanelAccount"));
    assert!(families.contains(&"BtPanelSite"));
    assert!(families.contains(&"BtPanelLog"));
    // No salt column on the legacy schema: the legacy algorithm form.
    let account = payload["artifacts"]
        .as_array()
        .expect("artifacts")
        .iter()
        .find(|a| a["family"] == "BtPanelAccount")
        .expect("account");
    assert!(account["attrs"]["passwordAlgorithm"]
        .as_str()
        .expect("algorithm")
        .contains("legacy"));
    assert_eq!(
        payload["timelineEvents"].as_array().expect("events").len(),
        1
    );
    // No hash material anywhere in the payload.
    assert!(!payload
        .to_string()
        .contains("21232f297a57a5a743894a0e4a801fc3"));
}

#[test]
fn modern_panel_db_account_with_bt0x_format() {
    // panel 9.x stores the real login account in db/panel.db with a
    // "BT-0x:"-prefixed proprietary password format and a salt column.
    let data = synthetic_db(
        "CREATE TABLE users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT, password TEXT, login_ip TEXT, login_time TEXT,
            phone TEXT, email TEXT, salt TEXT
        );
        INSERT INTO users (username, password, salt)
        VALUES ('eswr2ymq', 'BT-0x:/hMAW4hiSRDp90jJkKGOo1p5kGWs9EpXci',
                'BT-0x:w4eGRckcAS4ocwBOQ/7u352sN/TlO4zeUt');",
    );
    let (status, payload, _) = run_plugin("db/panel.db", &data);
    assert_eq!(status, MeowStatus::Ok);
    let payload = payload.expect("payload");
    let artifact = &payload["artifacts"][0];
    assert_eq!(artifact["family"], "BtPanelAccount");
    assert_eq!(artifact["title"], "eswr2ymq");
    assert_eq!(artifact["attrs"]["hasPasswordHash"], true);
    assert_eq!(
        artifact["attrs"]["passwordAlgorithm"],
        "BT-0x proprietary (panel 9.x format)"
    );
    let text = payload.to_string();
    assert!(!text.contains("hMAW4hiSRDp90jJkKGOo1p5kGWs9EpXci"));
    assert!(!text.contains("w4eGRckcAS4ocwBOQ"));
}

#[test]
fn modern_firewall_new_table_is_parsed() {
    let data = synthetic_db(
        "CREATE TABLE firewall_new (
            id INTEGER PRIMARY KEY AUTOINCREMENT, protocol TEXT, ports TEXT,
            types TEXT, address TEXT, brief TEXT, addtime TEXT, chain TEXT
        );
        INSERT INTO firewall_new (protocol, ports, types, address, brief, addtime)
        VALUES ('tcp', '8080', 'drop', '5.6.7.8', 'block scanner', '2025-06-02 12:00:00');",
    );
    let (status, payload, _) = run_plugin("db/firewall.db", &data);
    assert_eq!(status, MeowStatus::Ok);
    let payload = payload.expect("payload");
    let artifact = &payload["artifacts"][0];
    assert_eq!(artifact["family"], "BtPanelFirewall");
    assert_eq!(artifact["title"], "drop tcp/8080");
    assert_eq!(artifact["attrs"]["policy"], "drop");
    assert_eq!(artifact["attrs"]["sourceIp"], "5.6.7.8");
    assert_eq!(artifact["attrs"]["addtimeLocal"], "2025-06-02T12:00:00");
}

#[test]
fn missing_tables_are_skipped_silently() {
    let data = synthetic_db("CREATE TABLE unrelated (id INTEGER);");
    let (status, payload, _) = run_plugin("db/site.db", &data);
    assert_eq!(status, MeowStatus::Ok);
    let payload = payload.expect("payload");
    assert_eq!(payload["artifacts"].as_array().expect("artifacts").len(), 0);
}
