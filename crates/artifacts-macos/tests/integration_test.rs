//! Integration tests for the artifacts-macos crate.
//!
//! Tests all 7 macOS artifact parsers with synthetic fixtures,
//! verifies binary and XML plist parsing, and checks Spotlight
//! handles missing tables gracefully.

use rusqlite::Connection;

// ─────────────────────────────────────────────────────────────────────────────
// 1. All 7 parsers accept their synthetic fixtures
// ─────────────────────────────────────────────────────────────────────────────

// ── fsevents: parse_fsevents_log ────────────────────────────────────────────

/// Build a minimal FSEvents log test file.
fn build_fsevents_test_data() -> Vec<u8> {
    let mut data = Vec::new();
    // Magic: "1SLD"
    data.extend_from_slice(b"1SLD");
    data.resize(32, 0);

    // Timestamp at offset 16
    let ts: u64 = 1_705_276_800;
    data[16..24].copy_from_slice(&ts.to_be_bytes());

    // Event 1: Created file
    let e1_flags: u32 = 0x0100 | 0x20000; // Created + IsFile
    data.extend_from_slice(&e1_flags.to_le_bytes());
    data.extend_from_slice(&[0u8; 4]);
    data.extend_from_slice(b"/Users/test/Documents/new_file.txt");
    data.push(0);

    // Event 2: Modified file
    let e2_flags: u32 = 0x1000 | 0x20000; // Modified + IsFile
    data.extend_from_slice(&e2_flags.to_le_bytes());
    data.extend_from_slice(&[0u8; 4]);
    data.extend_from_slice(b"/Users/test/Documents/modified.doc");
    data.push(0);

    // Event 3: Removed dir
    let e3_flags: u32 = 0x0200 | 0x40000; // Removed + IsDir
    data.extend_from_slice(&e3_flags.to_le_bytes());
    data.extend_from_slice(&[0u8; 4]);
    data.extend_from_slice(b"/Users/test/old_folder");
    data.push(0);

    data
}

#[test]
fn fsevents_parses_synthetic_fixture() {
    let data = build_fsevents_test_data();
    let events = artifacts_macos::parse_fsevents_log(&data).expect("should parse FSEvents log");
    assert!(!events.is_empty(), "should find at least one FSEvent");
    // All paths should start with '/'
    for ev in &events {
        assert!(
            ev.path.starts_with('/'),
            "path should start with /: {}",
            ev.path
        );
    }
}

// ── launch_services: parse_launch_services_plist ────────────────────────────

#[test]
fn launch_services_parses_xml_fixture() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>LSHandlers</key>
    <array>
        <dict>
            <key>LSHandlerURLScheme</key>
            <string>http</string>
            <key>LSHandlerRoleAll</key>
            <string>com.apple.Safari</string>
        </dict>
        <dict>
            <key>LSHandlerURLScheme</key>
            <string>mailto</string>
            <key>LSHandlerRoleViewer</key>
            <string>com.apple.mail</string>
        </dict>
        <dict>
            <key>LSHandlerContentType</key>
            <string>com.adobe.pdf</string>
            <key>LSHandlerRoleAll</key>
            <string>com.apple.Preview</string>
        </dict>
    </array>
</dict>
</plist>"#;

    let services = artifacts_macos::parse_launch_services_plist(xml.as_bytes())
        .expect("should parse Launch Services plist");
    assert!(!services.is_empty(), "should have at least one service");

    let safari = services.iter().find(|s| s.bundle_id.contains("Safari"));
    assert!(safari.is_some(), "should find Safari handler");
    assert_eq!(safari.unwrap().kind, "URLHandler");

    let preview = services.iter().find(|s| s.bundle_id.contains("Preview"));
    assert!(preview.is_some(), "should find Preview handler");
    assert_eq!(preview.unwrap().kind, "ContentHandler");
}

// ── plist: binary and XML plist ─────────────────────────────────────────────

/// Build a minimal valid binary plist with a string key-value pair.
fn build_minimal_bplist() -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"bplist00");

    // obj0: key "CFBundleName"
    buf.push(0x5C);
    buf.extend_from_slice(b"CFBundleName");
    let obj0_off = 8u32;

    // obj1: val "Safari"
    buf.push(0x56);
    buf.extend_from_slice(b"Safari");
    let obj1_off = obj0_off + 13;

    // obj2: dict with 1 entry
    buf.push(0xD1);
    buf.extend_from_slice(&0u32.to_be_bytes());
    buf.extend_from_slice(&1u32.to_be_bytes());
    let obj2_off = obj1_off + 7;

    let num_objects: u32 = 3;
    let ot_start = buf.len() as u32;
    buf.extend_from_slice(&obj0_off.to_be_bytes());
    buf.extend_from_slice(&obj1_off.to_be_bytes());
    buf.extend_from_slice(&obj2_off.to_be_bytes());

    // Trailer (32 bytes)
    buf.extend_from_slice(&[0u8; 5]);
    buf.push(0);
    buf.push(4); // offset int size
    buf.push(4); // object ref size
    buf.extend_from_slice(&(num_objects as u64).to_be_bytes());
    buf.extend_from_slice(&(2u64).to_be_bytes()); // top object
    buf.extend_from_slice(&(ot_start as u64).to_be_bytes());

    buf
}

#[test]
fn binary_plist_parses_minimal_fixture() {
    let data = build_minimal_bplist();
    let entries = artifacts_macos::parse_binary_plist(&data, "/test/Info.plist")
        .expect("should parse binary plist");
    assert!(!entries.is_empty(), "should have entries");
    let found = entries.iter().find(|e| e.key == "CFBundleName");
    assert!(found.is_some(), "should find CFBundleName key");
    assert_eq!(found.unwrap().value, "Safari");
}

#[test]
fn xml_plist_parses_fixture() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.apple.Safari</string>
    <key>CFBundleVersion</key>
    <string>19618.1.15</string>
    <key>LSRequiresIPhoneOS</key>
    <true/>
    <key>MaxConnections</key>
    <integer>42</integer>
</dict>
</plist>"#;

    let entries = artifacts_macos::parse_xml_plist(xml.as_bytes(), "/test/Info.plist")
        .expect("should parse XML plist");
    assert_eq!(entries.len(), 4);

    let id = entries
        .iter()
        .find(|e| e.key == "CFBundleIdentifier")
        .unwrap();
    assert_eq!(id.value, "com.apple.Safari");
    assert_eq!(id.value_type, "string");

    let iphone = entries
        .iter()
        .find(|e| e.key == "LSRequiresIPhoneOS")
        .unwrap();
    assert_eq!(iphone.value, "true");
    assert_eq!(iphone.value_type, "boolean");

    let max = entries.iter().find(|e| e.key == "MaxConnections").unwrap();
    assert_eq!(max.value, "42");
    assert_eq!(max.value_type, "integer");
}

// ── quarantine: parse_quarantine_events ─────────────────────────────────────

fn build_quarantine_test_db() -> Vec<u8> {
    let tmp = tempfile::Builder::new()
        .suffix(".quarantine.db")
        .tempfile()
        .expect("create temp file");
    let tmp_path = tmp.path().to_path_buf();
    drop(tmp);

    let conn = Connection::open(&tmp_path).expect("open temp db");
    conn.execute_batch(
        "CREATE TABLE LSQuarantineEvent (
            LSQuarantineEventIdentifier TEXT PRIMARY KEY,
            LSQuarantineTimeStamp REAL,
            LSQuarantineAgentBundleIdentifier TEXT,
            LSQuarantineAgentName TEXT,
            LSQuarantineDataURLString TEXT,
            LSQuarantineOriginURLString TEXT,
            LSQuarantineSenderName TEXT,
            LSQuarantineOriginSenderName TEXT,
            LSQuarantineTypeNumber INTEGER
        );

        INSERT INTO LSQuarantineEvent VALUES
            ('uuid-1', 696902400.0, 'com.google.Chrome', 'Google Chrome',
             'https://example.com/file.dmg', '', 'Example Site', '', 0),
            ('uuid-2', 697000000.0, 'com.apple.Safari', 'Safari',
             'https://download.example.org/app.pkg', 'https://referrer.example.com',
             'Download Site', '', 0),
            ('uuid-3', 697100000.0, 'com.apple.mail', 'Mail',
             'https://cdn.example.net/doc.zip', '', '', '', 0);
        ",
    )
    .expect("create test db");

    drop(conn);
    std::fs::read(&tmp_path).expect("read temp db")
}

#[test]
fn quarantine_parses_synthetic_fixture() {
    let data = build_quarantine_test_db();
    let entries =
        artifacts_macos::parse_quarantine_events(&data).expect("should parse quarantine events");
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].url, "https://example.com/file.dmg");
    assert_eq!(entries[0].origin_bundle, "com.google.Chrome");
    assert_eq!(entries[0].agent, "Google Chrome");
    assert!(!entries[0].timestamp.is_empty());
    assert_ne!(entries[0].timestamp, "unknown");
}

// ── recent_items: parse_recent_items_plist ──────────────────────────────────

#[test]
fn recent_items_parses_xml_fixture() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>RecentFiles</key>
    <array>
        <dict>
            <key>Name</key>
            <string>report.pdf</string>
            <key>URL</key>
            <string>file:///Users/test/Documents/report.pdf</string>
            <key>Date</key>
            <date>2024-01-15T10:30:00Z</date>
        </dict>
        <dict>
            <key>Name</key>
            <string>budget.xlsx</string>
            <key>URL</key>
            <string>file:///Users/test/Documents/budget.xlsx</string>
        </dict>
    </array>
    <key>RecentServers</key>
    <array>
        <dict>
            <key>Name</key>
            <string>fileserver</string>
            <key>URL</key>
            <string>smb://fileserver.local/shared</string>
        </dict>
    </array>
    <key>RecentApplications</key>
    <array>
        <dict>
            <key>Name</key>
            <string>Safari</string>
            <key>URL</key>
            <string>file:///Applications/Safari.app</string>
        </dict>
    </array>
</dict>
</plist>"#;

    let items = artifacts_macos::parse_recent_items_plist(xml.as_bytes())
        .expect("should parse recent items");
    assert!(!items.is_empty(), "should have items");

    // At least the file entry
    let report = items.iter().find(|i| i.name == "report.pdf");
    assert!(report.is_some(), "should find report.pdf");
    assert_eq!(report.unwrap().kind, artifacts_macos::RecentItemKind::File);

    let app = items.iter().find(|i| i.name == "Safari");
    assert!(app.is_some(), "should find Safari app");
}

// ── spotlight: parse_spotlight_store ────────────────────────────────────────

fn build_spotlight_test_db() -> Vec<u8> {
    let tmp = tempfile::Builder::new()
        .suffix(".spotlight.db")
        .tempfile()
        .expect("create temp file");
    let tmp_path = tmp.path().to_path_buf();
    drop(tmp);

    let conn = Connection::open(&tmp_path).expect("open temp db");
    conn.execute_batch(
        "CREATE TABLE kMDItem (
            kMDItemPath TEXT,
            kMDItemDisplayName TEXT,
            kMDItemKind TEXT,
            kMDItemContentType TEXT,
            kMDItemFSCreationDate REAL,
            kMDItemFSContentChangeDate REAL,
            kMDItemAuthors TEXT
        );

        INSERT INTO kMDItem VALUES
            ('/Users/test/Documents/report.pdf', 'report.pdf', 'PDF document', 'com.adobe.pdf', 696902400.0, 697000000.0, 'John Doe'),
            ('/Users/test/Pictures/photo.jpg', 'photo.jpg', 'JPEG image', 'public.jpeg', 696800000.0, 696900000.0, '');
        ",
    )
    .expect("create test db");

    drop(conn);
    std::fs::read(&tmp_path).expect("read temp db")
}

#[test]
fn spotlight_parses_synthetic_fixture() {
    let data = build_spotlight_test_db();
    let entries =
        artifacts_macos::parse_spotlight_store(&data).expect("should parse spotlight store");
    assert!(!entries.is_empty(), "should find entries");
    assert!(
        !entries[0].display_name.is_empty(),
        "display name should not be empty"
    );
}

// ── unified_log: parse_tracev3 ──────────────────────────────────────────────

fn build_tracev3_test_data() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"tracev3");
    data.resize(256, 0);

    let chunk_start = data.len();
    data.push(0x13);
    data.push(0x60);
    data.push(0x00);
    data.push(0x00);
    let chunk_len: u32 = 128;
    data.extend_from_slice(&chunk_len.to_le_bytes());
    let mach_ts: u64 = 0x1000_0000_0000_0000;
    data.extend_from_slice(&mach_ts.to_le_bytes());

    data.resize(data.len() + 32, 0);

    // Process name "kernel\0"
    let proc_pos = data.len() - 20;
    data[proc_pos] = b'k';
    data[proc_pos + 1] = b'e';
    data[proc_pos + 2] = b'r';
    data[proc_pos + 3] = b'n';
    data[proc_pos + 4] = b'e';
    data[proc_pos + 5] = b'l';
    data[proc_pos + 6] = 0;

    data.extend_from_slice(b"System boot completed successfully");
    data.push(0);

    while data.len() < chunk_start + chunk_len as usize + 64 {
        data.push(0);
    }

    data
}

#[test]
fn unified_log_parses_synthetic_fixture() {
    let data = build_tracev3_test_data();
    let entries = artifacts_macos::parse_tracev3(&data).expect("should parse tracev3");
    assert!(!entries.is_empty(), "should find at least one log entry");
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Binary plist + XML plist both parse
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn both_plist_formats_parse() {
    // Binary plist
    let bplist_data = build_minimal_bplist();
    let b_entries = artifacts_macos::parse_binary_plist(&bplist_data, "/test/b.plist")
        .expect("binary plist should parse");
    assert!(!b_entries.is_empty());

    // XML plist
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Safari</string>
</dict>
</plist>"#;
    let x_entries = artifacts_macos::parse_xml_plist(xml.as_bytes(), "/test/x.plist")
        .expect("XML plist should parse");
    assert!(!x_entries.is_empty());

    // Both should extract the same key-value pair
    let b_name = b_entries.iter().find(|e| e.key == "CFBundleName").unwrap();
    let x_name = x_entries.iter().find(|e| e.key == "CFBundleName").unwrap();
    assert_eq!(b_name.value, x_name.value);
    assert_eq!(b_name.value, "Safari");
}

#[test]
fn binary_plist_rejects_non_plist() {
    assert!(artifacts_macos::parse_binary_plist(b"garbage data here\n", "/test").is_err());
}

#[test]
fn xml_plist_parses_empty_dict() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
</dict>
</plist>"#;
    let entries = artifacts_macos::parse_xml_plist(xml.as_bytes(), "/test/empty.plist")
        .expect("should parse empty plist");
    assert!(entries.is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Spotlight SQLite handles missing table gracefully
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn spotlight_handles_empty_database() {
    let tmp = tempfile::Builder::new()
        .suffix(".empty.db")
        .tempfile()
        .expect("create temp file");
    let tmp_path = tmp.path().to_path_buf();
    drop(tmp);

    let conn = Connection::open(&tmp_path).expect("open temp db");
    // Create zero tables — completely empty database
    drop(conn);

    let data = std::fs::read(&tmp_path).expect("read temp db");
    // Should not panic; should handle gracefully
    let result = artifacts_macos::parse_spotlight_store(&data);
    // Either returns Ok with empty vec, or Err — both are graceful
    match result {
        Ok(entries) => assert!(entries.is_empty(), "empty DB should yield empty entries"),
        Err(_) => {} // Error is also graceful
    }
}

#[test]
fn spotlight_handles_missing_expected_table() {
    let tmp = tempfile::Builder::new()
        .suffix(".wrong.db")
        .tempfile()
        .expect("create temp file");
    let tmp_path = tmp.path().to_path_buf();
    drop(tmp);

    let conn = Connection::open(&tmp_path).expect("open temp db");
    // Create a table that is NOT a Spotlight table
    conn.execute_batch(
        "CREATE TABLE unrelated (id INTEGER, data TEXT);
         INSERT INTO unrelated VALUES (1, 'hello'), (2, 'world');
         CREATE TABLE another (x INTEGER);
         INSERT INTO another VALUES (100);
        ",
    )
    .expect("create tables");
    drop(conn);

    let data = std::fs::read(&tmp_path).expect("read temp db");
    let result = artifacts_macos::parse_spotlight_store(&data);

    // Should not panic. If it returns entries, they are from generic parsing.
    if let Ok(entries) = result {
        // Entries might come from generic column scans. That's fine.
        // We just want to ensure no panic occurs.
        let _ = entries;
    }
}

#[test]
fn spotlight_rejects_empty_input() {
    let result = artifacts_macos::parse_spotlight_store(&[]);
    assert!(result.is_err());
}
