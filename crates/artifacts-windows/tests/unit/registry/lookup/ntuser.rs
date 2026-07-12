use crate::registry::lookup::*;
use crate::registry::tests::txlog_fixture::{build_synthetic_log1, SyntheticEntry};
use crate::registry::tests::*;

#[test]
fn test_rot13_decode_basic() {
    assert_eq!(
        rot13_decode("P:\\Jvaqbjf\\Flfgrz32\\abgrcnq.rkr"),
        "C:\\Windows\\System32\\notepad.exe"
    );
    assert_eq!(rot13_decode("Hello"), "Uryyb");
    assert_eq!(rot13_decode("Uryyb"), "Hello");
    assert_eq!(rot13_decode("123"), "123");
    assert_eq!(rot13_decode("!@#"), "!@#");
}

#[test]
fn test_rot13_decode_roundtrip() {
    // ROT13 is its own inverse — decoding twice yields the original.
    let original = "C:\\Users\\Admin\\Desktop\\calc.exe";
    let encoded = rot13_decode(original);
    assert_ne!(original, encoded, "encoded should differ from original");
    assert_eq!(
        rot13_decode(&encoded),
        original,
        "roundtrip should restore original"
    );

    let mixed = "Hello123!@#World";
    assert_eq!(rot13_decode(&rot13_decode(mixed)), mixed);
}

#[test]
fn windows_filetime_converts_to_rfc3339() {
    let ft = 133_600_000_000_000_000u64;
    let ts = windows_filetime_to_rfc3339(ft).expect("valid FILETIME");
    assert!(
        ts.starts_with("2024-") || ts.starts_with("2025-"),
        "timestamp {ts} should be in the 2024-2025 range"
    );
}

#[test]
fn windows_filetime_zero_returns_none() {
    assert_eq!(windows_filetime_to_rfc3339(0), None);
}

#[test]
fn test_empty_userassist_key() {
    let data = empty_hive("NTUSER");
    let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
    assert!(info.run_keys.is_empty());
    assert!(info.recent_docs.is_empty());
    assert!(info.ua_entries.is_empty());
    assert!(info.typed_urls.is_empty());
    assert!(info.word_wheel_query.is_empty());
    assert!(info.mount_points.is_empty());
    assert!(info.default_browser.is_none());
}

#[test]
fn extract_ntuser_run_keys() {
    let mut data = empty_hive("NTUSER");
    write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
    write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
    write_nk(
        &mut data,
        0x400,
        "Windows",
        &[("CurrentVersion", 0x500)],
        &[],
    );
    write_nk(&mut data, 0x500, "CurrentVersion", &[("Run", 0x600)], &[]);
    write_nk(&mut data, 0x600, "Run", &[], &[0x700, 0x780]);
    write_string_value(
        &mut data,
        0x700,
        "OneDrive",
        "C:\\Program Files\\Microsoft OneDrive\\OneDrive.exe /background",
        0x1000,
    );
    write_string_value(
        &mut data,
        0x780,
        "SecurityHealth",
        "%ProgramFiles%\\Windows Defender\\MSASCuiL.exe",
        0x1100,
    );

    let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
    assert_eq!(info.run_keys.len(), 2);
    let od = info
        .run_keys
        .iter()
        .find(|k| k.value_name == "OneDrive")
        .unwrap();
    assert!(od.command.contains("OneDrive.exe"));
    assert_eq!(
        od.key_path,
        "Software\\Microsoft\\Windows\\CurrentVersion\\Run"
    );
}

#[test]
fn extract_ntuser_run_once() {
    let mut data = empty_hive("NTUSER");
    write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
    write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
    write_nk(
        &mut data,
        0x400,
        "Windows",
        &[("CurrentVersion", 0x500)],
        &[],
    );
    write_nk(
        &mut data,
        0x500,
        "CurrentVersion",
        &[("RunOnce", 0x600)],
        &[],
    );
    write_nk(&mut data, 0x600, "RunOnce", &[], &[0x700]);
    write_string_value(
        &mut data,
        0x700,
        "Setup",
        "C:\\Windows\\Setup.exe /silent",
        0x1000,
    );

    let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
    assert_eq!(info.run_keys.len(), 1);
    assert_eq!(info.run_keys[0].value_name, "Setup");
    assert_eq!(
        info.run_keys[0].key_path,
        "Software\\Microsoft\\Windows\\CurrentVersion\\RunOnce"
    );
}

#[test]
fn extract_ntuser_recent_docs() {
    let mut data = empty_hive("NTUSER");
    write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
    write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
    write_nk(
        &mut data,
        0x400,
        "Windows",
        &[("CurrentVersion", 0x500)],
        &[],
    );
    write_nk(
        &mut data,
        0x500,
        "CurrentVersion",
        &[("Explorer", 0x600)],
        &[],
    );
    write_nk(&mut data, 0x600, "Explorer", &[("RecentDocs", 0x700)], &[]);
    write_nk(&mut data, 0x700, "RecentDocs", &[(".pdf", 0x800)], &[]);
    write_nk(&mut data, 0x800, ".pdf", &[], &[0x900, 0x980, 0xa00]);

    let mru_list = make_mru_list_ex(&[1, 0]);
    let doc0 = make_recent_doc_binary("report.pdf");
    let doc1 = make_recent_doc_binary("invoice.pdf");

    write_binary_value(&mut data, 0x900, "MRUListEx", &mru_list, 0x1200);
    write_binary_value(&mut data, 0x980, "0", &doc0, 0x1300);
    write_binary_value(&mut data, 0xa00, "1", &doc1, 0x1400);

    let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
    assert_eq!(info.recent_docs.len(), 2);
    // MRUListEx [1, 0] means index 1 is most recent
    assert_eq!(info.recent_docs[0].file_name, "invoice.pdf");
    assert_eq!(info.recent_docs[0].extension, ".pdf");
    assert_eq!(info.recent_docs[1].file_name, "report.pdf");
}

#[test]
fn test_userassist_extraction() {
    let mut data = empty_hive("NTUSER");
    write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
    write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
    write_nk(
        &mut data,
        0x400,
        "Windows",
        &[("CurrentVersion", 0x500)],
        &[],
    );
    write_nk(
        &mut data,
        0x500,
        "CurrentVersion",
        &[("Explorer", 0x600)],
        &[],
    );
    write_nk(&mut data, 0x600, "Explorer", &[("UserAssist", 0x700)], &[]);
    write_nk(
        &mut data,
        0x700,
        "UserAssist",
        &[("{CEBFF5CD-ACE2-4F4F-9178-9926F41749EA}", 0x800)],
        &[],
    );
    write_nk(
        &mut data,
        0x800,
        "{CEBFF5CD-ACE2-4F4F-9178-9926F41749EA}",
        &[("Count", 0x900)],
        &[],
    );
    write_nk(&mut data, 0x900, "Count", &[], &[0xa00, 0xb00]);

    let encrypted = "P:\\Jvaqbjf\\Flfgrz32\\abgrcnq.rkr";
    let ft: u64 = 133_600_000_000_000_000;
    // run_count=42, session_id=1, focus_time_ms=1500
    let ua1 = make_user_assist_binary(42, 1, 1500, ft);
    write_binary_value(&mut data, 0xa00, encrypted, &ua1, 0x1200);

    let encrypted2 = "P:\\Hfref\\Grfg\\Qrfxgbc\\pnyp.rkr";
    // run_count=7, session_id=2, focus_time_ms=300
    let ua2 = make_user_assist_binary(7, 2, 300, ft + 86_400_000_000_000);
    write_binary_value(&mut data, 0xb00, encrypted2, &ua2, 0x1300);

    let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
    assert_eq!(info.ua_entries.len(), 2);

    let notepad = info
        .ua_entries
        .iter()
        .find(|e| e.executable_path.contains("notepad"))
        .unwrap();
    assert_eq!(notepad.run_count, 42);
    assert_eq!(notepad.session_id, 1);
    assert_eq!(notepad.focus_time_ms, 1500);
    assert!(notepad.last_run.is_some());

    let calc = info
        .ua_entries
        .iter()
        .find(|e| e.executable_path.contains("calc"))
        .unwrap();
    assert_eq!(calc.run_count, 7);
    assert_eq!(calc.session_id, 2);
    assert_eq!(calc.focus_time_ms, 300);
}

#[test]
fn extract_ntuser_typed_urls() {
    let mut data = empty_hive("NTUSER");
    write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
    write_nk(
        &mut data,
        0x300,
        "Microsoft",
        &[("Internet Explorer", 0x400)],
        &[],
    );
    write_nk(
        &mut data,
        0x400,
        "Internet Explorer",
        &[("TypedURLs", 0x500)],
        &[],
    );
    write_nk(&mut data, 0x500, "TypedURLs", &[], &[0x600, 0x680, 0x700]);

    write_string_value(
        &mut data,
        0x600,
        "url1",
        "https://forensics.example.com",
        0x1000,
    );
    write_string_value(&mut data, 0x680, "url2", "https://github.com", 0x1100);
    write_string_value(&mut data, 0x700, "url3", "https://www.google.com", 0x1200);

    let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
    assert_eq!(info.typed_urls.len(), 3);
    assert_eq!(info.typed_urls[0], "https://forensics.example.com");
    assert_eq!(info.typed_urls[1], "https://github.com");
    assert_eq!(info.typed_urls[2], "https://www.google.com");
}

#[test]
fn extract_ntuser_word_wheel_query() {
    let mut data = empty_hive("NTUSER");
    write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
    write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
    write_nk(
        &mut data,
        0x400,
        "Windows",
        &[("CurrentVersion", 0x500)],
        &[],
    );
    write_nk(
        &mut data,
        0x500,
        "CurrentVersion",
        &[("Explorer", 0x600)],
        &[],
    );
    write_nk(
        &mut data,
        0x600,
        "Explorer",
        &[("WordWheelQuery", 0x700)],
        &[],
    );
    write_nk(
        &mut data,
        0x700,
        "WordWheelQuery",
        &[],
        &[0x800, 0x880, 0x900],
    );

    let wwq_mru = make_mru_list_ex(&[1, 0]);
    write_binary_value(&mut data, 0x800, "MRUListEx", &wwq_mru, 0x1000);
    write_string_value(&mut data, 0x880, "0", "forensics", 0x1100);
    write_string_value(&mut data, 0x900, "1", "evidence", 0x1200);

    let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
    assert_eq!(info.word_wheel_query.len(), 2);
    // MRUListEx [1, 0] -> index 1 is most recent
    assert_eq!(info.word_wheel_query[0], "evidence");
    assert_eq!(info.word_wheel_query[1], "forensics");
}

#[test]
fn extract_ntuser_mount_points() {
    let mut data = empty_hive("NTUSER");
    write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
    write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
    write_nk(
        &mut data,
        0x400,
        "Windows",
        &[("CurrentVersion", 0x500)],
        &[],
    );
    write_nk(
        &mut data,
        0x500,
        "CurrentVersion",
        &[("Explorer", 0x600)],
        &[],
    );
    write_nk(
        &mut data,
        0x600,
        "Explorer",
        &[("MountPoints2", 0x700)],
        &[],
    );
    write_nk(
        &mut data,
        0x700,
        "MountPoints2",
        &[
            ("C", 0x800),
            ("D", 0x900),
            ("{ecf5d85e-1234-5678-abcd-123456789abc}", 0xa00),
        ],
        &[],
    );
    write_nk(&mut data, 0x800, "C", &[], &[]);
    write_nk(&mut data, 0x900, "D", &[], &[]);
    write_nk(
        &mut data,
        0xa00,
        "{ecf5d85e-1234-5678-abcd-123456789abc}",
        &[],
        &[],
    );

    let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
    assert_eq!(info.mount_points.len(), 3);

    let c = info
        .mount_points
        .iter()
        .find(|m| m.drive_letter.as_deref() == Some("C:"))
        .unwrap();
    assert!(c.volume_guid.is_none());

    let guid = info
        .mount_points
        .iter()
        .find(|m| m.volume_guid.as_deref() == Some("{ecf5d85e-1234-5678-abcd-123456789abc}"))
        .unwrap();
    assert!(guid.drive_letter.is_none());
}

#[test]
fn extract_ntuser_combined() {
    // Run + RecentDocs + UserAssist in one hive.
    let mut data = empty_hive("NTUSER");
    write_nk(&mut data, 0x020, "NTUSER", &[("Software", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
    write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
    write_nk(
        &mut data,
        0x400,
        "Windows",
        &[("CurrentVersion", 0x600)],
        &[],
    );
    write_nk(
        &mut data,
        0x600,
        "CurrentVersion",
        &[("Run", 0x700), ("Explorer", 0x800)],
        &[],
    );
    // Run
    write_nk(&mut data, 0x700, "Run", &[], &[0x780]);
    write_string_value(&mut data, 0x780, "OneDrive", "C:\\OneDrive.exe /bg", 0x3000);
    // Explorer
    write_nk(
        &mut data,
        0x800,
        "Explorer",
        &[("RecentDocs", 0x900), ("UserAssist", 0xa00)],
        &[],
    );
    // RecentDocs
    write_nk(&mut data, 0x900, "RecentDocs", &[(".txt", 0xd00)], &[]);
    write_nk(&mut data, 0xd00, ".txt", &[], &[0xd80, 0xdc0]);
    let mru = make_mru_list_ex(&[0]);
    let doc = make_recent_doc_binary("notes.txt");
    write_binary_value(&mut data, 0xd80, "MRUListEx", &mru, 0x3100);
    write_binary_value(&mut data, 0xdc0, "0", &doc, 0x3200);
    // UserAssist
    write_nk(&mut data, 0xa00, "UserAssist", &[("{GUID}", 0xe00)], &[]);
    write_nk(&mut data, 0xe00, "{GUID}", &[("Count", 0xf00)], &[]);
    write_nk(&mut data, 0xf00, "Count", &[], &[0xf80]);
    let ua = make_user_assist_binary(99, 3, 5000, 133_600_000_000_000_000);
    write_binary_value(
        &mut data,
        0xf80,
        "P:\\Hfref\\Grfg\\Qrfxgbc\\pnyp.rkr",
        &ua,
        0x3300,
    );

    let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
    assert_eq!(info.run_keys.len(), 1);
    assert_eq!(info.recent_docs.len(), 1);
    assert_eq!(info.ua_entries.len(), 1);
    assert_eq!(info.run_keys[0].value_name, "OneDrive");
    assert_eq!(info.recent_docs[0].file_name, "notes.txt");
    assert!(info.ua_entries[0].executable_path.contains("calc"));
    assert_eq!(info.ua_entries[0].run_count, 99);
}

#[test]
fn extract_ntuser_combined_group2() {
    // WordWheelQuery + MountPoints2 + TypedURLs in one hive.
    let mut data = empty_hive("NTUSER");
    write_nk(&mut data, 0x020, "NTUSER", &[("Software", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
    write_nk(
        &mut data,
        0x300,
        "Microsoft",
        &[("Windows", 0x400), ("Internet Explorer", 0x500)],
        &[],
    );
    write_nk(
        &mut data,
        0x400,
        "Windows",
        &[("CurrentVersion", 0x600)],
        &[],
    );
    write_nk(
        &mut data,
        0x600,
        "CurrentVersion",
        &[("Explorer", 0x800)],
        &[],
    );
    // Explorer
    write_nk(
        &mut data,
        0x800,
        "Explorer",
        &[("WordWheelQuery", 0x900), ("MountPoints2", 0xa00)],
        &[],
    );
    // WordWheelQuery
    write_nk(&mut data, 0x900, "WordWheelQuery", &[], &[0x980, 0x9c0]);
    let wwq_mru = make_mru_list_ex(&[0]);
    write_string_value(&mut data, 0x980, "0", "search term", 0x3000);
    write_binary_value(&mut data, 0x9c0, "MRUListEx", &wwq_mru, 0x3100);
    // MountPoints2
    write_nk(&mut data, 0xa00, "MountPoints2", &[("E", 0xb00)], &[]);
    write_nk(&mut data, 0xb00, "E", &[], &[]);
    // IE TypedURLs
    write_nk(
        &mut data,
        0x500,
        "Internet Explorer",
        &[("TypedURLs", 0xc00)],
        &[],
    );
    write_nk(&mut data, 0xc00, "TypedURLs", &[], &[0xc80]);
    write_string_value(&mut data, 0xc80, "url1", "https://example.com", 0x3200);

    let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
    assert_eq!(info.word_wheel_query.len(), 1);
    assert_eq!(info.mount_points.len(), 1);
    assert_eq!(info.typed_urls.len(), 1);
    assert_eq!(info.word_wheel_query[0], "search term");
    assert_eq!(info.mount_points[0].drive_letter.as_deref(), Some("E:"));
    assert_eq!(info.typed_urls[0], "https://example.com");
}

#[test]
fn extract_ntuser_handles_missing_keys() {
    let mut data = empty_hive("NTUSER");
    write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Software", &[("Unrelated", 0x300)], &[]);
    write_nk(&mut data, 0x300, "Unrelated", &[], &[]);

    let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
    assert!(info.run_keys.is_empty());
    assert!(info.recent_docs.is_empty());
    assert!(info.ua_entries.is_empty());
    assert!(info.typed_urls.is_empty());
    assert!(info.word_wheel_query.is_empty());
    assert!(info.mount_points.is_empty());
}

#[test]
fn ntuser_hive_with_txlog_overrides_run_key_command() {
    // Build an NTUSER hive with a single Run key.
    let mut data = empty_hive("NTUSER");
    write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
    write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
    write_nk(
        &mut data,
        0x400,
        "Windows",
        &[("CurrentVersion", 0x500)],
        &[],
    );
    write_nk(&mut data, 0x500, "CurrentVersion", &[("Run", 0x600)], &[]);
    write_nk(&mut data, 0x600, "Run", &[], &[0x700]);
    write_string_value(&mut data, 0x700, "Malware", "C:\\temp\\old.exe", 0x1000);

    let txlog_bytes = build_synthetic_log1(&[SyntheticEntry {
        operation: 2, // SetValue
        sequence_number: 200,
        timestamp: Some(0x01DB_A100_0000_0000),
        key_path:
            "\\Registry\\User\\S-1-5-21-123\\Software\\Microsoft\\Windows\\CurrentVersion\\Run"
                .to_string(),
        value_name: Some("Malware".to_string()),
        data_before: Some(encode_utf16le("C:\\temp\\old.exe")),
        data_after: Some(encode_utf16le("C:\\temp\\new.exe")),
    }]);

    let info =
        extract_ntuser_fields_with_txlog(&data, "Users/Test/NTUSER.DAT", &txlog_bytes).unwrap();

    assert_eq!(info.run_keys.len(), 1);
    assert_eq!(info.run_keys[0].value_name, "Malware");
    assert_eq!(info.run_keys[0].command, "C:\\temp\\new.exe");
    assert!(
        info.run_keys[0].timestamp.is_some(),
        "Run key should have timestamp from txlog"
    );
    assert!(info.txlog_applied);
}

#[test]
fn extract_run_mru_from_fixture() {
    let mut data = empty_hive("NTUSER");
    write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
    write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
    write_nk(
        &mut data,
        0x400,
        "Windows",
        &[("CurrentVersion", 0x500)],
        &[],
    );
    write_nk(
        &mut data,
        0x500,
        "CurrentVersion",
        &[("Explorer", 0x600)],
        &[],
    );
    write_nk(&mut data, 0x600, "Explorer", &[("RunMRU", 0x700)], &[]);
    write_nk(&mut data, 0x700, "RunMRU", &[], &[0x800, 0x880, 0x900]);
    write_string_value(&mut data, 0x800, "MRUList", "acb", 0x4000);
    write_string_value(&mut data, 0x880, "a", "cmd.exe", 0x4100);
    write_string_value(&mut data, 0x900, "b", "powershell.exe", 0x4200);

    let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
    assert_eq!(info.run_mru.len(), 2);
    let a = info.run_mru.iter().find(|e| e.value_name == "a").unwrap();
    assert_eq!(a.command, "cmd.exe");
    assert_eq!(
        a.source_key_path,
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\RunMRU"
    );
    let b = info.run_mru.iter().find(|e| e.value_name == "b").unwrap();
    assert_eq!(b.command, "powershell.exe");
}

#[test]
fn extract_open_save_mru_from_fixture() {
    let mut data = empty_hive("NTUSER");
    write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
    write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
    write_nk(
        &mut data,
        0x400,
        "Windows",
        &[("CurrentVersion", 0x500)],
        &[],
    );
    write_nk(
        &mut data,
        0x500,
        "CurrentVersion",
        &[("Explorer", 0x600)],
        &[],
    );
    write_nk(&mut data, 0x600, "Explorer", &[("ComDlg32", 0x700)], &[]);
    write_nk(
        &mut data,
        0x700,
        "ComDlg32",
        &[("OpenSavePidlMRU", 0x800)],
        &[],
    );
    write_nk(&mut data, 0x800, "OpenSavePidlMRU", &[("txt", 0x900)], &[]);
    write_nk(&mut data, 0x900, "txt", &[], &[0x980, 0xa00]);
    let mru_list = make_mru_list_ex(&[0]);
    let pidl = make_pidl_blob_with_string("report.txt");
    write_binary_value(&mut data, 0x980, "MRUListEx", &mru_list, 0x4000);
    write_binary_value(&mut data, 0xa00, "0", &pidl, 0x4100);

    let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
    assert_eq!(info.open_save_mru.len(), 1);
    let entry = &info.open_save_mru[0];
    assert_eq!(entry.extension, "txt");
    assert_eq!(entry.value_name, "0");
    assert_eq!(entry.file_name, "report.txt");
    assert_eq!(entry.raw_pidl_hex, hex::encode(&pidl));
    assert!(entry.source_key_path.ends_with("OpenSavePidlMRU\\txt"));
}

#[test]
fn extract_last_visited_mru_from_fixture() {
    let mut data = empty_hive("NTUSER");
    write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
    write_nk(&mut data, 0x300, "Microsoft", &[("Windows", 0x400)], &[]);
    write_nk(
        &mut data,
        0x400,
        "Windows",
        &[("CurrentVersion", 0x500)],
        &[],
    );
    write_nk(
        &mut data,
        0x500,
        "CurrentVersion",
        &[("Explorer", 0x600)],
        &[],
    );
    write_nk(&mut data, 0x600, "Explorer", &[("ComDlg32", 0x700)], &[]);
    write_nk(
        &mut data,
        0x700,
        "ComDlg32",
        &[("LastVisitedPidlMRU", 0x800)],
        &[],
    );
    write_nk(&mut data, 0x800, "LastVisitedPidlMRU", &[], &[0x880, 0x900]);
    let mru_list = make_mru_list_ex(&[0]);
    let pidl = make_pidl_blob_with_string("C:\\Users\\Test\\Documents");
    write_binary_value(&mut data, 0x880, "MRUListEx", &mru_list, 0x4000);
    write_binary_value(&mut data, 0x900, "0", &pidl, 0x4100);

    let info = extract_ntuser_fields(&data, "Users/Test/NTUSER.DAT").unwrap();
    assert_eq!(info.last_visited_mru.len(), 1);
    let entry = &info.last_visited_mru[0];
    assert_eq!(entry.value_name, "0");
    assert_eq!(entry.path, "C:\\Users\\Test\\Documents");
    assert_eq!(entry.raw_pidl_hex, hex::encode(&pidl));
    assert!(entry.source_key_path.ends_with("LastVisitedPidlMRU"));
}

#[test]
fn extract_appcompat_layers_from_ntuser_fixture() {
    let mut data = empty_hive("NTUSER");
    write_nk(&mut data, 0x20, "NTUSER", &[("Software", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Software", &[("Microsoft", 0x300)], &[]);
    write_nk(&mut data, 0x300, "Microsoft", &[("Windows NT", 0xb00)], &[]);
    write_nk(
        &mut data,
        0xb00,
        "Windows NT",
        &[("CurrentVersion", 0xc00)],
        &[],
    );
    write_nk(
        &mut data,
        0xc00,
        "CurrentVersion",
        &[("AppCompatFlags", 0xd00)],
        &[],
    );
    write_nk(
        &mut data,
        0xd00,
        "AppCompatFlags",
        &[("Layers", 0xe00)],
        &[],
    );
    write_nk(&mut data, 0xe00, "Layers", &[], &[0x1500, 0x1580]);
    write_string_value(&mut data, 0x1500, "calc.exe", "WIN7RTM", 0x4000);
    write_string_value(
        &mut data,
        0x1580,
        "C:\\Windows\\System32\\notepad.exe",
        "WINXPSP3 RUNASADMIN",
        0x4100,
    );

    let entries =
        extract_appcompat_layers_from_ntuser_hive(&data, "Users/Test/NTUSER.DAT").unwrap();

    assert_eq!(entries.len(), 2);
    let calc = entries
        .iter()
        .find(|e| e.executable_path == "calc.exe")
        .unwrap();
    assert_eq!(calc.layer_string, "WIN7RTM");
    assert_eq!(
        calc.source_key_path,
        "Software\\Microsoft\\Windows NT\\CurrentVersion\\AppCompatFlags\\Layers"
    );
    assert_eq!(calc.source_hive_path, "Users/Test/NTUSER.DAT");

    let notepad = entries
        .iter()
        .find(|e| e.executable_path.contains("notepad.exe"))
        .unwrap();
    assert_eq!(notepad.layer_string, "WINXPSP3 RUNASADMIN");
}

fn make_pidl_blob_with_string(s: &str) -> Vec<u8> {
    let mut blob = vec![0x14, 0x00, 0x1f, 0x00, 0xe0, 0x00]; // synthetic PIDL prefix
    let utf16: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
    blob.extend_from_slice(&utf16);
    blob.extend_from_slice(&[0x00, 0x00]); // null terminator
    blob.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // trailing padding
    blob
}
