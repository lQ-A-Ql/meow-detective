use super::*;
use chrono::{DateTime, Utc};
use domain::{
    CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind, EntryType, FileEntry, FileEntryId,
};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo, datasource_repo::DataSourceRepo, file_repo::FileRepo,
};
use persistence_sqlite::{open_in_memory, runner};
use rusqlite::{params, Connection};
use std::{cell::Cell, collections::HashMap, io::Read, io::Write, path::Path, rc::Rc};
use tempfile::TempDir;
use testing::{builders::registry, fixtures};
use transport::dto::AnalysisParseStatusDto;

fn file(id: &str, path: &str, size: u64) -> FileEntry {
    FileEntry {
        id: FileEntryId(id.to_string()),
        parent_id: None,
        data_source_id: DataSourceId("ds".to_string()),
        path: path.to_string(),
        name: Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
        entry_type: EntryType::File,
        size: Some(size),
        ext: Path::new(path)
            .extension()
            .map(|ext| ext.to_string_lossy().to_string()),
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}

fn file_with_ds(id: &str, data_source_id: &DataSourceId, path: &str, size: u64) -> FileEntry {
    let mut entry = file(id, path, size);
    entry.data_source_id = data_source_id.clone();
    entry
}

fn setup_case_db() -> (Connection, TempDir, DataSourceId) {
    let conn = open_in_memory().unwrap();
    runner::run_all(&conn).unwrap();
    let case = CaseMeta {
        id: CaseId("case-analysis".to_string()),
        name: "Analysis Test".to_string(),
        number: None,
        examiner: None,
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    CaseRepo::new(&conn).create(&case).unwrap();

    let tmp = TempDir::new().unwrap();
    let ds_id = DataSourceId("ds-analysis".to_string());
    let source = DataSource {
        id: ds_id.clone(),
        name: "logical".to_string(),
        kind: DataSourceKind::LogicalDirectory,
        source_path: tmp.path().to_path_buf(),
        imported_at: chrono::Utc::now(),
        provenance: domain::DataSourceProvenance::unknown(),
    };
    DataSourceRepo::new(&conn)
        .insert(&CaseId(case.id.0), &source)
        .unwrap();

    (conn, tmp, ds_id)
}

fn sqlite_db_bytes(build: impl FnOnce(&Connection)) -> Vec<u8> {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("source.sqlite");
    {
        let db = Connection::open(&db_path).unwrap();
        build(&db);
    }
    std::fs::read(db_path).unwrap()
}

fn chromium_time(value: &str) -> i64 {
    let dt = DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc);
    (dt.timestamp() + 11_644_473_600) * 1_000_000 + i64::from(dt.timestamp_subsec_micros())
}

fn unix_microseconds(value: &str) -> i64 {
    let dt = DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc);
    dt.timestamp() * 1_000_000 + i64::from(dt.timestamp_subsec_micros())
}

fn gzip_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

fn chromium_history_bytes(url: &str, title: &str, target_path: &str) -> Vec<u8> {
    sqlite_db_bytes(|db| {
        db.execute_batch(
            "CREATE TABLE urls (
                    id INTEGER PRIMARY KEY,
                    url TEXT NOT NULL,
                    title TEXT,
                    visit_count INTEGER,
                    last_visit_time INTEGER
                );
                CREATE TABLE visits (
                    id INTEGER PRIMARY KEY,
                    url INTEGER NOT NULL,
                    visit_time INTEGER
                );
                CREATE TABLE downloads (
                    id INTEGER PRIMARY KEY,
                    tab_url TEXT,
                    target_path TEXT,
                    start_time INTEGER,
                    total_bytes INTEGER
                );",
        )
        .unwrap();
        db.execute(
            "INSERT INTO urls (id, url, title, visit_count, last_visit_time)
                 VALUES (1, ?1, ?2, 3, ?3)",
            params![url, title, chromium_time("2024-01-02T03:04:05Z")],
        )
        .unwrap();
        db.execute(
            "INSERT INTO visits (id, url, visit_time) VALUES (1, 1, ?1)",
            params![chromium_time("2024-01-02T03:04:05Z")],
        )
        .unwrap();
        db.execute(
            "INSERT INTO downloads (id, tab_url, target_path, start_time, total_bytes)
                 VALUES (1, ?1, ?2, ?3, 4096)",
            params![url, target_path, chromium_time("2024-01-03T04:05:06Z")],
        )
        .unwrap();
    })
}

fn firefox_places_bytes() -> Vec<u8> {
    sqlite_db_bytes(|db| {
        db.execute_batch(
            "CREATE TABLE moz_places (
                    id INTEGER PRIMARY KEY,
                    url TEXT NOT NULL,
                    title TEXT,
                    visit_count INTEGER,
                    last_visit_date INTEGER
                );
                CREATE TABLE moz_historyvisits (
                    id INTEGER PRIMARY KEY,
                    place_id INTEGER NOT NULL,
                    visit_date INTEGER
                );",
        )
        .unwrap();
        db.execute(
            "INSERT INTO moz_places (id, url, title, visit_count, last_visit_date)
                 VALUES (1, 'https://mozilla.example/', 'Firefox Example', 2, ?1)",
            params![unix_microseconds("2024-01-04T05:06:07Z")],
        )
        .unwrap();
        db.execute(
            "INSERT INTO moz_historyvisits (id, place_id, visit_date) VALUES (1, 1, ?1)",
            params![unix_microseconds("2024-01-04T05:06:07Z")],
        )
        .unwrap();
    })
}

fn sample_email_bytes() -> Vec<u8> {
    b"Date: Tue, 02 Jan 2024 03:04:05 +0000\r\nFrom: alice@example.com\r\nTo: bob@example.com, carol@example.com\r\nSubject: Quarterly evidence note\r\nMessage-ID: <msg-1@example.com>\r\nContent-Disposition: attachment; filename=\"evidence.txt\"\r\n\r\nThis is the first line of the message body.\r\nThis is the second line.\r\n".to_vec()
}

#[derive(Clone)]
struct BoundedBytesProbe {
    bytes: Vec<u8>,
    requested_limits: Rc<std::cell::RefCell<Vec<usize>>>,
    full_reader_calls: Rc<Cell<usize>>,
}

struct CountingRead {
    inner: std::io::Cursor<Vec<u8>>,
    bytes_read: Rc<Cell<usize>>,
}

impl CountingRead {
    fn new(bytes: Vec<u8>, bytes_read: Rc<Cell<usize>>) -> Self {
        Self {
            inner: std::io::Cursor::new(bytes),
            bytes_read,
        }
    }
}

impl Read for CountingRead {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.bytes_read.set(self.bytes_read.get() + n);
        Ok(n)
    }
}

impl BoundedBytesProbe {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            requested_limits: Rc::new(std::cell::RefCell::new(Vec::new())),
            full_reader_calls: Rc::new(Cell::new(0)),
        }
    }

    fn read_header(&self, max_bytes: usize) -> Vec<u8> {
        self.requested_limits.borrow_mut().push(max_bytes);
        self.bytes[..self.bytes.len().min(max_bytes)].to_vec()
    }

    fn open_full_reader(&self) -> Box<dyn Read> {
        self.full_reader_calls.set(self.full_reader_calls.get() + 1);
        panic!("test path must use bounded header bytes, not full reader");
    }

    fn requested_limits(&self) -> Vec<usize> {
        self.requested_limits.borrow().clone()
    }

    fn full_reader_calls(&self) -> usize {
        self.full_reader_calls.get()
    }
}

#[test]
fn exact_length_magic_signatures_are_detected() {
    let files = vec![
        file("pdf", "doc.bin", 4),
        file("zip", "archive.bin", 4),
        file("reg", "NTUSER.DAT", 4),
    ];
    let classifications = classify_files_by_magic(&files, 100, |id| -> Result<Vec<u8>, String> {
        match id.0.as_str() {
            "pdf" => Ok(b"%PDF".to_vec()),
            "zip" => Ok(b"PK\x03\x04".to_vec()),
            "reg" => Ok(b"regf".to_vec()),
            _ => Ok(Vec::new()),
        }
    });

    let detected = classifications
        .iter()
        .flat_map(|cat| cat.files.iter())
        .map(|file| file.file_type.as_str())
        .collect::<Vec<_>>();
    assert!(detected.contains(&"PDF"));
    assert!(detected.contains(&"ZIP"));
    assert!(detected.contains(&"REG"));
}

#[test]
fn system_info_is_not_fabricated_without_parsers() {
    let (conn, _tmp, _ds_id) = setup_case_db();
    let info =
        extract_system_info_for_case(&conn, |_file_id, _max_bytes| -> Result<Vec<u8>, String> {
            panic!("no files should be read when hives are missing")
        });
    assert_eq!(info.status, AnalysisParseStatusDto::NotParsed);
    assert!(info.computer_name.is_none());
    assert!(info.os_version.is_none());
    assert!(info.build_number.is_none());
    assert!(info.boot_history.is_empty());
    assert!(info
        .warnings
        .iter()
        .any(|warning| warning.contains("未在证据文件目录中发现 Windows/System32/config/SYSTEM")));
    assert!(info.provenance.iter().any(|item| {
        item.parser == REGISTRY_SYSTEM_PARSER && item.status == AnalysisParseStatusDto::Unavailable
    }));
}

#[test]
fn summary_does_not_emit_fake_default_facts() {
    let (conn, _tmp, _ds_id) = setup_case_db();
    let info =
        extract_system_info_for_case(&conn, |_file_id, _max_bytes| -> Result<Vec<u8>, String> {
            Err("unexpected read".to_string())
        });
    let summary = generate_analysis_summary(&info, &[]);

    assert!(summary.contains("未解析"));
    assert!(!summary.contains("FORENSICS-PC"));
    assert!(!summary.contains("Windows 10"));
    assert!(!summary.contains("19045"));
}

#[test]
fn classification_uses_file_id_reader_and_limits_sample() {
    let files = vec![file("a", "a.exe", 2), file("b", "b.pdf", 4)];
    let mut requested = Vec::new();
    let classifications = classify_files_by_magic(&files, 1, |id| -> Result<Vec<u8>, String> {
        requested.push(id.0.clone());
        Ok(b"MZ".to_vec())
    });

    assert_eq!(requested, vec!["a"]);
    let count: usize = classifications.iter().map(|cat| cat.files.len()).sum();
    assert_eq!(count, 1);
}

#[test]
fn malformed_registry_hive_presence_keeps_system_fields_empty_with_provenance() {
    let (conn, _tmp, ds_id) = setup_case_db();
    FileRepo::new(&conn)
        .insert_batch(&[
            file_with_ds("system", &ds_id, "Windows/System32/config/SYSTEM", 4096),
            file_with_ds("software", &ds_id, "Windows/System32/config/SOFTWARE", 4096),
        ])
        .unwrap();

    let info =
        extract_system_info_for_case(&conn, |file_id, _max_bytes| match file_id.0.as_str() {
            "system" | "software" => Ok(b"regf".to_vec()),
            other => Err(format!("unexpected file id {other}")),
        });

    assert_eq!(info.status, AnalysisParseStatusDto::NotParsed);
    assert!(info.computer_name.is_none());
    assert!(info.os_version.is_none());
    assert!(info.registered_owner.is_none());
    assert!(info.provenance.iter().any(|item| {
        item.parser == REGISTRY_SYSTEM_PARSER
            && item.data_source_id == ds_id.0
            && item.artifact_path == "Windows/System32/config/SYSTEM"
            && item.status == AnalysisParseStatusDto::NotParsed
            && item
                .warnings
                .iter()
                .any(|warning| warning.contains("registry hive shorter than base block"))
    }));
}

#[test]
fn registry_hive_fields_are_parsed_with_field_provenance() {
    let (conn, _tmp, ds_id) = setup_case_db();
    let system_hive = std::fs::read(fixtures::tiny_registry_system_hive())
        .expect("read tiny SYSTEM registry fixture");
    let software_hive = std::fs::read(fixtures::tiny_registry_software_hive())
        .expect("read tiny SOFTWARE registry fixture");
    FileRepo::new(&conn)
        .insert_batch(&[
            file_with_ds(
                "system",
                &ds_id,
                "Windows/System32/config/SYSTEM",
                system_hive.len() as u64,
            ),
            file_with_ds(
                "software",
                &ds_id,
                "Windows/System32/config/SOFTWARE",
                software_hive.len() as u64,
            ),
        ])
        .unwrap();

    let info = extract_system_info_for_case(&conn, |file_id, max_bytes| match file_id.0.as_str() {
        "system" => Ok(system_hive[..system_hive.len().min(max_bytes)].to_vec()),
        "software" => Ok(software_hive[..software_hive.len().min(max_bytes)].to_vec()),
        other => Err(format!("unexpected file id {other}")),
    });

    assert_eq!(info.status, AnalysisParseStatusDto::Parsed);
    assert_eq!(
        info.computer_name.as_deref(),
        Some(registry::SYSTEM_COMPUTER_NAME)
    );
    assert_eq!(
        info.os_version.as_deref(),
        Some("Forensics Fixture OS 24H2")
    );
    assert_eq!(
        info.build_number.as_deref(),
        Some(registry::SOFTWARE_CURRENT_BUILD)
    );
    assert_eq!(
        info.registered_owner.as_deref(),
        Some(registry::SOFTWARE_REGISTERED_OWNER)
    );
    assert_eq!(
        info.product_id.as_deref(),
        Some(registry::SOFTWARE_PRODUCT_ID)
    );
    assert_eq!(info.timezone.as_deref(), Some(registry::SYSTEM_TIMEZONE));
    assert!(info
        .install_date
        .as_deref()
        .is_some_and(|value| value.starts_with("2023-")));
    assert!(info.field_provenance.iter().any(|field| {
        field.field == "computerName"
            && field.value_name == "ComputerName"
            && field.key_path == "ControlSet001\\Control\\ComputerName\\ComputerName"
            && field.hive_path == "Windows/System32/config/SYSTEM"
            && field.parser == REGISTRY_SYSTEM_PARSER
    }));
    assert!(info.field_provenance.iter().any(|field| {
        field.field == "osVersion"
            && field.value_name == "ProductName"
            && field.key_path == "Microsoft\\Windows NT\\CurrentVersion"
            && field.hive_path == "Windows/System32/config/SOFTWARE"
            && field.parser == REGISTRY_SOFTWARE_PARSER
    }));
    assert!(info.provenance.iter().any(|item| {
        item.parser == REGISTRY_SYSTEM_PARSER
            && item.status == AnalysisParseStatusDto::Parsed
            && item.data_source_id == ds_id.0
    }));
    assert!(info.provenance.iter().any(|item| {
        item.parser == REGISTRY_SOFTWARE_PARSER
            && item.status == AnalysisParseStatusDto::Parsed
            && item.data_source_id == ds_id.0
    }));

    let summary = generate_analysis_summary(&info, &[]);
    assert!(summary.contains(registry::SYSTEM_COMPUTER_NAME));
    assert!(summary.contains("Forensics Fixture OS 24H2"));
    assert!(!summary.contains("FORENSICS-PC"));
}

#[test]
fn corrupted_registry_hive_records_warning_without_facts() {
    let (conn, _tmp, ds_id) = setup_case_db();
    FileRepo::new(&conn)
        .insert_batch(&[file_with_ds(
            "system",
            &ds_id,
            "Windows/System32/config/SYSTEM",
            4,
        )])
        .unwrap();

    let info =
        extract_system_info_for_case(&conn, |_file_id, _max_bytes| -> Result<Vec<u8>, String> {
            Ok(b"BAD!".to_vec())
        });

    assert!(info.computer_name.is_none());
    assert!(info.boot_history.is_empty());
    assert!(info
        .warnings
        .iter()
        .any(|warning| warning.contains("不含 regf 头")));
}

#[test]
fn malformed_evtx_source_is_not_parsed_and_generates_no_boot_records() {
    let (conn, _tmp, ds_id) = setup_case_db();
    FileRepo::new(&conn)
        .insert_batch(&[file_with_ds(
            "system-evtx",
            &ds_id,
            "Windows/System32/winevt/Logs/System.evtx",
            8192,
        )])
        .unwrap();

    let info =
        extract_system_info_for_case(&conn, |_file_id, _max_bytes| -> Result<Vec<u8>, String> {
            Ok(vec![0x45, 0x6c, 0x66, 0x46])
        });

    assert!(info.boot_history.is_empty());
    assert!(info
        .warnings
        .iter()
        .all(|warning| !warning.contains("EVTX parser initialization failed")));
    assert!(info.provenance.iter().any(|item| {
        item.parser == EVTX_BOOT_SHUTDOWN_PARSER
            && item.status == AnalysisParseStatusDto::NotParsed
            && item
                .warnings
                .iter()
                .any(|warning| warning.contains("EVTX parser initialization failed"))
    }));
}

#[test]
fn classification_carries_read_error_provenance() {
    let files = vec![file("bad", "bad.bin", 10)];
    let classifications = classify_files_by_magic(&files, 10, |_id| {
        Err("unsupported data source kind".to_string())
    });

    let other = classifications
        .iter()
        .find(|cat| cat.category == "Other")
        .expect("unclassified bucket should be present");
    assert_eq!(
        other.files[0].provenance.status,
        AnalysisParseStatusDto::Unavailable
    );
    assert!(other.files[0]
        .provenance
        .warnings
        .iter()
        .any(|warning| warning.contains("unsupported data source kind")));
    assert!(other
        .warnings
        .iter()
        .any(|warning| warning.contains("unsupported data source kind")));
}

#[test]
fn evidence_discovery_maps_registry_evtx_prefetch_lnk_paths_to_categories() {
    let (conn, _tmp, ds_id) = setup_case_db();
    FileRepo::new(&conn)
        .insert_batch(&[
            file_with_ds("system", &ds_id, "Windows/System32/config/SYSTEM", 10),
            file_with_ds(
                "evtx",
                &ds_id,
                "Windows/System32/winevt/Logs/System.evtx",
                20,
            ),
            file_with_ds("pf", &ds_id, "Windows/Prefetch/CMD.EXE-12345678.pf", 30),
            file_with_ds(
                "lnk",
                &ds_id,
                "Users/alice/AppData/Roaming/Microsoft/Windows/Recent/app.lnk",
                40,
            ),
            file_with_ds("reg", &ds_id, "Users/alice/NTUSER.DAT", 50),
            file_with_ds(
                "history",
                &ds_id,
                "Users/alice/AppData/Local/Google/Chrome/User Data/Default/History",
                60,
            ),
            file_with_ds("email-eml", &ds_id, "Users/alice/Inbox/message.eml", 70),
            file_with_ds("email-emlx", &ds_id, "Users/alice/Inbox/message.emlx", 80),
        ])
        .unwrap();

    let candidates = discover_evidence_candidates(&conn).unwrap();
    assert_eq!(candidates.get("SystemInformation").map(Vec::len), Some(2));
    assert_eq!(candidates.get("Registry").map(Vec::len), Some(2));
    assert_eq!(candidates.get("BrowserHistory").map(Vec::len), Some(1));
    assert_eq!(candidates.get("Email").map(Vec::len), Some(2));
    assert_eq!(candidates.get("EventLogs").map(Vec::len), Some(1));
    assert_eq!(candidates.get("ProgramExecution").map(Vec::len), Some(1));
    assert_eq!(candidates.get("UserActivity").map(Vec::len), Some(1));
}

#[test]
fn targeted_evidence_scan_can_use_bounded_bytes_reader_without_full_open() {
    let (conn, _tmp, ds_id) = setup_case_db();
    FileRepo::new(&conn)
        .insert_batch(&[file_with_ds(
            "lnk",
            &ds_id,
            "Users/alice/AppData/Roaming/Microsoft/Windows/Recent/app.lnk",
            64,
        )])
        .unwrap();

    let probe = BoundedBytesProbe::new(vec![0u8; 64]);
    let full_reader_probe = probe.clone();
    let _full_reader_path = move || full_reader_probe.open_full_reader();
    let reader_probe = probe.clone();

    let stats = crate::artifact_service::run_targeted_evidence_scan(
        &conn,
        "case-analysis",
        &["UserActivity"],
        |file_id| {
            assert_eq!(file_id.0, "lnk");
            let bytes = reader_probe
                .read_header(infrastructure::constants::ARTIFACT_FILE_LIMIT_BYTES as usize);
            Ok::<Box<dyn Read>, crate::artifact_service::ArtifactServiceError>(Box::new(
                std::io::Cursor::new(bytes),
            ))
        },
    )
    .unwrap();

    assert_eq!(stats.candidate_count, 1);
    assert_eq!(stats.scanned_count, 1);
    assert_eq!(
        probe.requested_limits(),
        vec![infrastructure::constants::ARTIFACT_FILE_LIMIT_BYTES as usize]
    );
    assert_eq!(probe.full_reader_calls(), 0);
}

#[test]
fn run_analysis_extraction_can_use_bounded_bytes_reader_without_full_open() {
    let (conn, _tmp, ds_id) = setup_case_db();
    let email = sample_email_bytes();
    FileRepo::new(&conn)
        .insert_batch(&[file_with_ds(
            "email",
            &ds_id,
            "Users/alice/Mail/message.eml",
            email.len() as u64,
        )])
        .unwrap();

    let probe = BoundedBytesProbe::new(email);
    let full_reader_probe = probe.clone();
    let _full_reader_path = move || full_reader_probe.open_full_reader();
    let reader_probe = probe.clone();

    let run = run_analysis_extraction(&conn, "case-analysis", &["Email"], |file_id| {
        assert_eq!(file_id.0, "email");
        let bytes = reader_probe.read_header(MAX_ANALYSIS_SOURCE_BYTES);
        Ok::<Box<dyn Read>, String>(Box::new(std::io::Cursor::new(bytes)))
    })
    .unwrap();

    assert_eq!(run.status, AnalysisParseStatusDto::Parsed);
    assert_eq!(run.scanned_count, 1);
    assert_eq!(run.artifact_count, 1);
    assert_eq!(probe.requested_limits(), vec![MAX_ANALYSIS_SOURCE_BYTES]);
    assert_eq!(probe.full_reader_calls(), 0);
}

#[test]
fn linux_analysis_extraction_uses_smaller_text_log_read_cap() {
    let (conn, _tmp, ds_id) = setup_case_db();
    let auth_log = b"Jan 01 00:00:01 host sudo: alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/id\n".to_vec();
    FileRepo::new(&conn)
        .insert_batch(&[file_with_ds(
            "auth",
            &ds_id,
            "var/log/auth.log",
            64 * 1024 * 1024,
        )])
        .unwrap();

    let mut source_bytes = auth_log;
    source_bytes.resize(64 * 1024 * 1024, b'x');
    let bytes_read = Rc::new(Cell::new(0usize));
    let bytes_read_probe = Rc::clone(&bytes_read);
    let run = run_analysis_extraction(&conn, "case-analysis", &["LinuxArtifacts"], |file_id| {
        assert_eq!(file_id.0, "auth");
        Ok::<Box<dyn Read>, String>(Box::new(CountingRead::new(
            source_bytes.clone(),
            Rc::clone(&bytes_read_probe),
        )))
    })
    .unwrap();

    assert_eq!(run.status, AnalysisParseStatusDto::Partial);
    assert_eq!(run.scanned_count, 1);
    assert!(run.artifact_count > 0);
    assert!(bytes_read.get() <= 16 * 1024 * 1024);
}

#[test]
fn run_analysis_extraction_extracts_registry_browser_email_and_persists() {
    let (conn, _tmp, ds_id) = setup_case_db();
    let system_hive = std::fs::read(fixtures::tiny_registry_system_hive()).unwrap();
    let software_hive = std::fs::read(fixtures::tiny_registry_software_hive()).unwrap();
    let chrome_history = chromium_history_bytes(
        "https://chrome.example/",
        "Chrome Example",
        "C:/Temp/chrome.bin",
    );
    let edge_history =
        chromium_history_bytes("https://edge.example/", "Edge Example", "C:/Temp/edge.bin");
    let firefox_places = firefox_places_bytes();
    let email = sample_email_bytes();

    FileRepo::new(&conn)
        .insert_batch(&[
            file_with_ds(
                "system",
                &ds_id,
                "Windows/System32/config/SYSTEM",
                system_hive.len() as u64,
            ),
            file_with_ds(
                "software",
                &ds_id,
                "Windows/System32/config/SOFTWARE",
                software_hive.len() as u64,
            ),
            file_with_ds(
                "chrome-history",
                &ds_id,
                "Users/alice/AppData/Local/Google/Chrome/User Data/Default/History",
                chrome_history.len() as u64,
            ),
            file_with_ds(
                "edge-history",
                &ds_id,
                "Users/alice/AppData/Local/Microsoft/Edge/User Data/Profile 1/History",
                edge_history.len() as u64,
            ),
            file_with_ds(
                "firefox-places",
                &ds_id,
                "Users/alice/AppData/Roaming/Mozilla/Firefox/Profiles/abc.default/places.sqlite",
                firefox_places.len() as u64,
            ),
            file_with_ds(
                "email",
                &ds_id,
                "Users/alice/Mail/message.eml",
                email.len() as u64,
            ),
        ])
        .unwrap();

    let mut contents = HashMap::new();
    contents.insert("system".to_string(), system_hive);
    contents.insert("software".to_string(), software_hive);
    contents.insert("chrome-history".to_string(), chrome_history);
    contents.insert("edge-history".to_string(), edge_history);
    contents.insert("firefox-places".to_string(), firefox_places);
    contents.insert("email".to_string(), email);

    let run = run_analysis_extraction(&conn, "case-analysis", &[], |file_id| {
        contents
            .get(&file_id.0)
            .cloned()
            .map(|bytes| Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read>)
            .ok_or_else(|| format!("missing bytes for {}", file_id.0))
    })
    .unwrap();

    assert_eq!(run.status, AnalysisParseStatusDto::Parsed);
    assert!(run.warnings.is_empty());
    assert_eq!(run.scanned_count, 6);
    assert_eq!(run.artifact_count, 16);
    assert_eq!(run.timeline_event_count, 6);

    let mut stmt = conn
        .prepare(
            "SELECT artifact_type, COUNT(*)
                 FROM artifacts
                 GROUP BY artifact_type
                 ORDER BY artifact_type",
        )
        .unwrap();
    let counts = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })
        .unwrap()
        .collect::<Result<HashMap<_, _>, _>>()
        .unwrap();
    assert_eq!(counts.get("RegistryValue").copied(), Some(8));
    assert_eq!(counts.get("RegistryHive").copied(), Some(2));
    assert_eq!(counts.get("BrowserHistory").copied(), Some(3));
    assert_eq!(counts.get("BrowserDownload").copied(), Some(2));
    assert_eq!(counts.get("EmailMessage").copied(), Some(1));

    let timeline_case_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM timeline_events WHERE case_id = 'case-analysis'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(timeline_case_count, 6);

    let registry = get_registry_extraction_summary(&conn, 0, 20).unwrap();
    assert_eq!(registry.total, 10);
    assert!(registry.values.iter().any(|value| {
        value.source_path == "Windows/System32/config/SYSTEM"
            && value.key_path == "ControlSet001\\Control\\ComputerName\\ComputerName"
            && value.value_name == "ComputerName"
            && value.data == registry::SYSTEM_COMPUTER_NAME
    }));

    let structured = get_registry_structured_summary(&conn).unwrap();
    assert_eq!(structured.status, AnalysisParseStatusDto::Parsed);
    let hive_names: Vec<&str> = structured
        .hive_overviews
        .iter()
        .map(|h| h.hive_name.as_str())
        .collect();
    assert!(hive_names.contains(&"SYSTEM"));
    assert!(hive_names.contains(&"SOFTWARE"));
    assert!(structured.sam_users.is_empty());
    assert!(structured.user_assist_entries.is_empty());

    let browser = get_browser_history_summary(&conn, 0, 20).unwrap();
    assert_eq!(browser.visit_total, 3);
    assert_eq!(browser.download_total, 2);
    assert!(browser.visits.iter().any(|visit| {
        visit.browser == "Chrome"
            && visit.profile == "Default"
            && visit.url == "https://chrome.example/"
            && visit.visit_count == 3
            && visit.visit_time.as_deref() == Some("2024-01-02T03:04:05+00:00")
    }));
    assert!(browser.visits.iter().any(|visit| {
        visit.browser == "Firefox"
            && visit.profile == "abc.default"
            && visit.url == "https://mozilla.example/"
    }));
    assert!(browser.downloads.iter().any(|download| {
        download.browser == "Edge"
            && download.profile == "Profile 1"
            && download.target_path == "C:/Temp/edge.bin"
            && download.total_bytes == 4096
    }));

    let email_summary = get_email_extraction_summary(&conn, 0, 20).unwrap();
    assert_eq!(email_summary.total, 1);
    let message = email_summary.messages.first().unwrap();
    assert_eq!(message.from, "alice@example.com");
    assert_eq!(
        message.to,
        vec![
            "bob@example.com".to_string(),
            "carol@example.com".to_string()
        ]
    );
    assert_eq!(message.subject, "Quarterly evidence note");
    assert_eq!(message.message_id, "<msg-1@example.com>");
    assert_eq!(message.attachments, vec!["evidence.txt".to_string()]);
    assert!(message
        .body_preview
        .contains("first line of the message body"));

    let second_run = run_analysis_extraction(&conn, "case-analysis", &[], |file_id| {
        contents
            .get(&file_id.0)
            .cloned()
            .map(|bytes| Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read>)
            .ok_or_else(|| format!("missing bytes for {}", file_id.0))
    })
    .unwrap();
    assert_eq!(second_run.scanned_count, 0);
    assert_eq!(second_run.artifact_count, 16);
    let artifact_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))
        .unwrap();
    let timeline_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM timeline_events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(artifact_count, 16);
    assert_eq!(timeline_count, 6);
}

#[test]
fn run_analysis_extraction_extracts_eventlogs_and_persists() {
    let (conn, _tmp, ds_id) = setup_case_db();
    let evtx_bytes = std::fs::read(fixtures::tiny_system_evtx()).unwrap();
    let mut contents: HashMap<String, Vec<u8>> = HashMap::new();
    contents.insert("system-evtx".to_string(), evtx_bytes);
    FileRepo::new(&conn)
        .insert_batch(&[file_with_ds(
            "system-evtx",
            &ds_id,
            "Windows/System32/winevt/Logs/System.evtx",
            1024,
        )])
        .unwrap();

    let run = run_analysis_extraction(&conn, "case-analysis", &["EventLogs"], |file_id| {
        contents
            .get(&file_id.0)
            .cloned()
            .map(|bytes| Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read>)
            .ok_or_else(|| format!("missing bytes for {}", file_id.0))
    })
    .unwrap();

    assert_eq!(run.scanned_count, 1);
    let artifact_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE case_id = 'case-analysis'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let timeline_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM timeline_events WHERE case_id = 'case-analysis'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(artifact_count > 0, "expected EVTX artifacts");
    assert_eq!(
        artifact_count, timeline_count,
        "each EVTX boot/shutdown event should produce one artifact and one timeline event"
    );
}

#[test]
fn run_analysis_extraction_extracts_linux_artifacts_and_persists() {
    let (conn, _tmp, ds_id) = setup_case_db();
    let bash_history = "#1700000000\nls -la /home\ncat /etc/hostname\n";
    let zsh_history = ": 1700000100:0;whoami\n";
    let fish_history = "- cmd: uname -a\n  when: 1700000200\n";
    let syslog = "Jan 15 10:30:00 ubuntu sshd[1234]: Accepted publickey for alice from 192.168.1.100 port 22\n";
    let authorized_keys = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAITestKey alice@example\n";
    let auth_rotated = gzip_bytes(
        b"Jan 15 10:31:00 ubuntu sudo: alice : TTY=pts/0 ; PWD=/home/alice ; USER=root ; COMMAND=/usr/bin/id\n",
    );
    let mut contents: HashMap<String, Vec<u8>> = HashMap::new();
    contents.insert("bash-history".to_string(), bash_history.as_bytes().to_vec());
    contents.insert("zsh-history".to_string(), zsh_history.as_bytes().to_vec());
    contents.insert("fish-history".to_string(), fish_history.as_bytes().to_vec());
    contents.insert("syslog".to_string(), syslog.as_bytes().to_vec());
    contents.insert(
        "authorized-keys".to_string(),
        authorized_keys.as_bytes().to_vec(),
    );
    contents.insert("auth-rotated".to_string(), auth_rotated);
    FileRepo::new(&conn)
        .insert_batch(&[
            file_with_ds(
                "bash-history",
                &ds_id,
                "home/alice/.bash_history",
                bash_history.len() as u64,
            ),
            file_with_ds(
                "zsh-history",
                &ds_id,
                "home/alice/.zsh_history",
                zsh_history.len() as u64,
            ),
            file_with_ds(
                "fish-history",
                &ds_id,
                "home/alice/.local/share/fish/fish_history",
                fish_history.len() as u64,
            ),
            file_with_ds("syslog", &ds_id, "var/log/syslog.1", syslog.len() as u64),
            file_with_ds(
                "authorized-keys",
                &ds_id,
                "home/alice/.ssh/authorized_keys",
                authorized_keys.len() as u64,
            ),
            file_with_ds("auth-rotated", &ds_id, "var/log/auth.log.1.gz", 96),
        ])
        .unwrap();

    let run = run_analysis_extraction(&conn, "case-analysis", &["LinuxArtifacts"], |file_id| {
        contents
            .get(&file_id.0)
            .cloned()
            .map(|bytes| Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read>)
            .ok_or_else(|| format!("missing bytes for {}", file_id.0))
    })
    .unwrap();

    assert_eq!(run.scanned_count, 6);
    let artifact_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE artifact_type = 'LinuxBashCommand'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(artifact_count > 0, "expected LinuxBashCommand artifacts");
    let journal_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE artifact_type = 'LinuxJournal'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let sudo_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE artifact_type = 'LinuxSudoEvent'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(journal_count > 0, "expected generic LinuxJournal text logs");
    let ssh_message: String = conn
        .query_row(
            "SELECT json_extract(attrs, '$.message')
             FROM artifacts
             WHERE artifact_type = 'LinuxJournal'
               AND json_extract(attrs, '$.sourcePath') = 'home/alice/.ssh/authorized_keys'
             LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(ssh_message, authorized_keys.trim());
    assert!(
        sudo_count > 0,
        "expected sudo artifact from gz rotated auth log"
    );

    let summary = get_linux_artifact_summary(&conn, 0, 200).unwrap();
    assert_eq!(summary.bash_command_count, artifact_count as u64);
    assert_eq!(
        summary.total_count,
        artifact_count as u64 + journal_count as u64 + sudo_count as u64
    );
    assert_eq!(summary.status, AnalysisParseStatusDto::Parsed);
    assert!(!summary.truncated);
    assert!((summary.coverage_ratio - 1.0).abs() < f32::EPSILON);
    let first = summary
        .bash_commands
        .iter()
        .find(|cmd| cmd.command == "ls -la /home")
        .expect("bash command should be present in summary");
    assert_eq!(first.source_path, "home/alice/.bash_history");
    assert!(first.timestamp.is_some());
    assert!(summary
        .bash_commands
        .iter()
        .any(|cmd| cmd.command == "whoami" && cmd.source_path.ends_with(".zsh_history")));
    assert!(summary
        .bash_commands
        .iter()
        .any(|cmd| cmd.command == "uname -a" && cmd.source_path.ends_with("fish_history")));
}

#[test]
fn get_linux_artifact_summary_reports_not_found_without_artifacts() {
    let (conn, _tmp, _ds_id) = setup_case_db();

    let summary = get_linux_artifact_summary(&conn, 0, 200).unwrap();

    assert_eq!(summary.status, AnalysisParseStatusDto::NotFound);
    assert_eq!(summary.total_count, 0);
    assert!(!summary.truncated);
    assert_eq!(summary.coverage_ratio, 0.0);
    assert!(summary.bash_commands.is_empty());
}

#[test]
fn discover_evidence_candidates_includes_linux_first_pass_paths() {
    let (conn, _tmp, ds_id) = setup_case_db();
    FileRepo::new(&conn)
        .insert_batch(&[
            file_with_ds("audit", &ds_id, "var/log/audit/audit.log.1.gz", 10),
            file_with_ds("syslog", &ds_id, "var/log/syslog.1", 10),
            file_with_ds("messages", &ds_id, "var/log/messages", 10),
            file_with_ds("kern", &ds_id, "var/log/kern.log.2.gz", 10),
            file_with_ds("zsh", &ds_id, "home/alice/.zsh_history", 10),
            file_with_ds(
                "fish",
                &ds_id,
                "home/alice/.local/share/fish/fish_history",
                10,
            ),
            file_with_ds("ssh", &ds_id, "home/alice/.ssh/authorized_keys", 10),
            file_with_ds(
                "systemd",
                &ds_id,
                "etc/systemd/system/update-checker.service",
                10,
            ),
        ])
        .unwrap();

    let candidates = discover_evidence_candidates(&conn).unwrap();
    let linux = candidates.get("LinuxArtifacts").unwrap();

    assert_eq!(linux.len(), 8);
    assert!(linux
        .iter()
        .any(|item| item.path.ends_with("audit.log.1.gz")));
    assert!(linux.iter().any(|item| item.path.ends_with(".zsh_history")));
    assert!(linux.iter().any(|item| item.path.ends_with("fish_history")));
    assert!(linux
        .iter()
        .any(|item| item.path.ends_with("authorized_keys")));
    assert!(linux
        .iter()
        .any(|item| item.path.ends_with("update-checker.service")));
}

#[test]
fn linux_summary_reports_candidates_without_artifacts_and_truncation() {
    let (conn, _tmp, ds_id) = setup_case_db();
    FileRepo::new(&conn)
        .insert_batch(&[
            file_with_ds(
                "large-auth",
                &ds_id,
                "var/log/auth.log",
                MAX_ANALYSIS_SOURCE_BYTES as u64 + 1,
            ),
            file_with_ds(
                "systemd",
                &ds_id,
                "etc/systemd/system/backdoor.service",
                128,
            ),
        ])
        .unwrap();

    let summary = get_linux_artifact_summary(&conn, 0, 200).unwrap();

    assert_eq!(summary.status, AnalysisParseStatusDto::CandidateFound);
    assert_eq!(summary.total_count, 0);
    assert!(summary.truncated);
    assert_eq!(summary.coverage_ratio, 0.0);
    assert!(summary
        .warnings
        .iter()
        .any(|warning| warning.contains("Found 2 Linux artifact candidate")));
    assert!(summary
        .warnings
        .iter()
        .any(|warning| warning.contains("128 MiB cap")));
    assert!(summary
        .warnings
        .iter()
        .any(|warning| warning.contains("do not yet have a structured")));
}

#[test]
fn evidence_summary_reports_candidate_found_without_parser_run() {
    let (conn, _tmp, ds_id) = setup_case_db();
    FileRepo::new(&conn)
        .insert_batch(&[file_with_ds(
            "system",
            &ds_id,
            "Windows/System32/config/SYSTEM",
            10,
        )])
        .unwrap();

    let summary = get_evidence_classification_summary(&conn).unwrap();
    let system = summary
        .categories
        .iter()
        .find(|category| category.category == "SystemInformation")
        .unwrap();
    assert_eq!(system.status, AnalysisParseStatusDto::CandidateFound);
    assert_eq!(system.file_count, 1);
    assert_eq!(system.artifact_count, 0);
    assert_eq!(system.sources[0].path, "Windows/System32/config/SYSTEM");
}

#[test]
fn evidence_summary_reports_parsed_when_artifacts_exist() {
    let (conn, _tmp, ds_id) = setup_case_db();
    FileRepo::new(&conn)
        .insert_batch(&[file_with_ds(
            "pf",
            &ds_id,
            "Windows/Prefetch/CMD.EXE-12345678.pf",
            10,
        )])
        .unwrap();
    conn.execute(
            "INSERT INTO artifacts
             (id, case_id, data_source_id, artifact_type, source_object_id, title, summary, attrs, created_at)
             VALUES ('artifact-1', 'case-analysis', ?1, 'Prefetch', 'pf', 'Prefetch: CMD.EXE', 'summary', '{}', '2026-01-01T00:00:00Z')",
            [&ds_id.0],
        )
        .unwrap();

    let summary = get_evidence_classification_summary(&conn).unwrap();
    let program = summary
        .categories
        .iter()
        .find(|category| category.category == "ProgramExecution")
        .unwrap();
    assert_eq!(program.status, AnalysisParseStatusDto::Parsed);
    assert_eq!(program.artifact_count, 1);
    assert_eq!(program.sources[0].status, AnalysisParseStatusDto::Parsed);
}
