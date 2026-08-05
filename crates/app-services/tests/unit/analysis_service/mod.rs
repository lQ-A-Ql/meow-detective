use super::provenance::{
    EVTX_BOOT_SHUTDOWN_PARSER, REGISTRY_SOFTWARE_PARSER, REGISTRY_SYSTEM_PARSER,
};
use super::*;
use chrono::{DateTime, Utc};
use domain::DataSourcePlatform::{Linux as L, Windows as W};
use domain::{
    Artifact, ArtifactId, CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind, EntryType,
    FileEntry, FileEntryId,
};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, case_repo::CaseRepo, datasource_repo::DataSourceRepo,
    file_repo::FileRepo,
};
use persistence_sqlite::{open_in_memory, runner};
use rusqlite::{params, Connection};
use std::{cell::Cell, collections::HashMap, io::Read, io::Write, path::Path, rc::Rc};
use tempfile::TempDir;
use testing::{builders::registry, fixtures};
use transport::dto::{AnalysisParseStatusDto, EvtxEventViewDto};

mod cancellation;

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

#[test]
fn registry_structured_summary_includes_physical_network_adapters() {
    let (conn, _tmp, data_source_id) = setup_case_db();
    let attrs = serde_json::from_value(serde_json::json!({
        "guid": "{ADAPTER-GUID}",
        "name": "Ethernet",
        "description": "Intel Ethernet Controller",
        "macAddress": "00:11:22:33:44:55",
        "ipAddresses": ["192.0.2.10"],
        "subnetMasks": ["255.255.255.0"],
        "gateways": ["192.0.2.1"],
        "dhcpEnabled": true,
        "dhcpServer": "192.0.2.2",
        "dnsServers": ["192.0.2.53"],
        "pnpInstanceId": "PCI\\VEN_8086&DEV_1234",
        "serviceName": "e1dexpress"
    }))
    .unwrap();
    let artifact = Artifact {
        id: ArtifactId("network-adapter-1".to_string()),
        family: "RegistryNetworkAdapter".to_string(),
        title: "Network Adapter: Ethernet".to_string(),
        summary: "physical adapter".to_string(),
        source_object_id: Some(FileEntryId("system-hive".to_string())),
        extractor_id: Some("registry.system.network.v1".to_string()),
        extractor_version: Some(ANALYSIS_EXTRACTOR_VERSION.to_string()),
        confidence: Some(0.85),
        source_attribution: Some("Windows/System32/config/SYSTEM".to_string()),
        created_at: Utc::now(),
        attrs,
    };
    ArtifactRepo::new(&conn)
        .insert_batch(&[artifact], "case-analysis", &data_source_id.0)
        .unwrap();

    let summary = get_registry_structured_summary(&conn).unwrap();

    assert_eq!(summary.status, AnalysisParseStatusDto::Parsed);
    assert_eq!(summary.network_adapters.len(), 1);
    let adapter = &summary.network_adapters[0];
    assert_eq!(adapter.name, "Ethernet");
    assert_eq!(adapter.ip_addresses, ["192.0.2.10"]);
    assert_eq!(adapter.subnet_masks, ["255.255.255.0"]);
    assert_eq!(
        adapter.pnp_instance_id.as_deref(),
        Some("PCI\\VEN_8086&DEV_1234")
    );
}

#[test]
fn evtx_summary_preserves_boot_event_timestamp_from_persisted_artifact() {
    let (conn, _tmp, data_source_id) = setup_case_db();
    let attrs = serde_json::from_value(serde_json::json!({
        "timestamp": "2026-07-22T01:02:03+00:00",
        "eventId": 13,
        "recordId": 42,
        "provider": "Microsoft-Windows-Kernel-General",
        "eventKind": "operatingSystemShutdown",
        "sourcePath": "Windows/System32/winevt/Logs/System.evtx",
        "note": "Windows entered the operating-system shutdown phase."
    }))
    .unwrap();
    let artifact = Artifact {
        id: ArtifactId("evtx-boot-1".to_string()),
        family: "EvtxBootShutdown".to_string(),
        title: "EVTX operatingSystemShutdown event 13".to_string(),
        summary: "shutdown phase".to_string(),
        source_object_id: Some(FileEntryId("system-evtx".to_string())),
        extractor_id: Some("evtx.structured".to_string()),
        extractor_version: Some(ANALYSIS_EXTRACTOR_VERSION.to_string()),
        confidence: Some(0.85),
        source_attribution: Some("Windows/System32/winevt/Logs/System.evtx".to_string()),
        created_at: Utc::now(),
        attrs,
    };
    ArtifactRepo::new(&conn)
        .insert_batch(&[artifact], "case-analysis", &data_source_id.0)
        .unwrap();

    let summary = get_evtx_event_summary(&conn, None, 0, 100).unwrap();

    assert_eq!(summary.boot_events.len(), 1);
    assert_eq!(
        summary.boot_events[0].timestamp,
        "2026-07-22T01:02:03+00:00"
    );
    assert_eq!(summary.boot_events[0].kind, "operatingSystemShutdown");
}

#[test]
fn evtx_summary_pages_the_requested_view_and_counts_the_full_dataset() {
    let (conn, _tmp, data_source_id) = setup_case_db();
    let artifacts = (0..3)
        .map(|index| Artifact {
            id: ArtifactId(format!("evtx-process-{index}")),
            family: "EvtxSecurityEvent".to_string(),
            title: format!("EVTX process event {index}"),
            summary: "process created".to_string(),
            source_object_id: Some(FileEntryId("security-evtx".to_string())),
            extractor_id: Some("evtx.structured".to_string()),
            extractor_version: Some(ANALYSIS_EXTRACTOR_VERSION.to_string()),
            confidence: Some(0.85),
            source_attribution: Some("Windows/System32/winevt/Logs/Security.evtx".to_string()),
            created_at: Utc::now(),
            attrs: serde_json::from_value(serde_json::json!({
                "timestamp": format!("2026-07-22T00:00:0{index}+00:00"),
                "eventId": 4688,
                "recordId": index + 1,
                "kind": "processCreated",
                "sourcePath": "Windows/System32/winevt/Logs/Security.evtx"
            }))
            .unwrap(),
        })
        .collect::<Vec<_>>();
    ArtifactRepo::new(&conn)
        .insert_batch(&artifacts, "case-analysis", &data_source_id.0)
        .unwrap();

    let first_page = get_evtx_event_summary(&conn, Some(EvtxEventViewDto::Process), 0, 1).unwrap();
    let second_page = get_evtx_event_summary(&conn, Some(EvtxEventViewDto::Process), 1, 1).unwrap();
    let third_page = get_evtx_event_summary(&conn, Some(EvtxEventViewDto::Process), 2, 1).unwrap();

    assert_eq!(first_page.page_total, 3);
    assert_eq!(first_page.process_execution_count, 3);
    assert_eq!(first_page.security_events.len(), 1);
    assert_eq!(first_page.security_events[0].record_id, Some(3));
    assert_eq!(second_page.security_events[0].record_id, Some(2));
    assert_eq!(third_page.security_events[0].record_id, Some(1));
    assert!(first_page.boot_events.is_empty());
    assert!(first_page.application_events.is_empty());
}

#[test]
fn evtx_summary_places_invalid_timestamps_after_valid_events() {
    let (conn, _tmp, data_source_id) = setup_case_db();
    let artifacts = [
        ("evtx-invalid", "unknown", 99),
        ("evtx-valid", "2026-07-22T00:00:00+00:00", 1),
    ]
    .into_iter()
    .map(|(id, timestamp, record_id)| Artifact {
        id: ArtifactId(id.to_string()),
        family: "EvtxSecurityEvent".to_string(),
        title: "EVTX process event".to_string(),
        summary: "process created".to_string(),
        source_object_id: Some(FileEntryId("security-evtx".to_string())),
        extractor_id: Some("evtx.structured".to_string()),
        extractor_version: Some(ANALYSIS_EXTRACTOR_VERSION.to_string()),
        confidence: Some(0.85),
        source_attribution: Some("Windows/System32/winevt/Logs/Security.evtx".to_string()),
        created_at: Utc::now(),
        attrs: serde_json::from_value(serde_json::json!({
            "timestamp": timestamp,
            "eventId": 4688,
            "recordId": record_id,
            "kind": "processCreated",
            "sourcePath": "Windows/System32/winevt/Logs/Security.evtx"
        }))
        .unwrap(),
    })
    .collect::<Vec<_>>();
    ArtifactRepo::new(&conn)
        .insert_batch(&artifacts, "case-analysis", &data_source_id.0)
        .unwrap();

    let summary = get_evtx_event_summary(&conn, Some(EvtxEventViewDto::Process), 0, 10).unwrap();

    assert_eq!(summary.security_events.len(), 2);
    assert_eq!(summary.security_events[0].record_id, Some(1));
    assert_eq!(summary.security_events[1].record_id, Some(99));
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
fn windows_build_number_includes_update_build_revision_when_available() {
    assert_eq!(
        super::system_info::format_full_build_number("19045", Some("6216")),
        "19045.6216"
    );
    assert_eq!(
        super::system_info::format_full_build_number("19045", None),
        "19045"
    );
    assert_eq!(
        super::system_info::format_full_build_number("19045.6216", Some("6216")),
        "19045.6216"
    );
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

    let run = run_analysis_extraction_with_reader_limits(
        &conn,
        "case-analysis",
        W,
        &["Email"],
        |file_id, read_limit| {
            assert_eq!(file_id.0, "email");
            let bytes = reader_probe.read_header(read_limit);
            Ok::<Box<dyn Read>, String>(Box::new(std::io::Cursor::new(bytes)))
        },
    )
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

    let probe = BoundedBytesProbe::new(auth_log);
    let reader_probe = probe.clone();
    let run = run_analysis_extraction_with_reader_limits(
        &conn,
        "case-analysis",
        L,
        &["LinuxArtifacts"],
        |file_id, read_limit| {
            assert_eq!(file_id.0, "auth");
            let bytes = reader_probe.read_header(read_limit);
            Ok::<Box<dyn Read>, String>(Box::new(std::io::Cursor::new(bytes)))
        },
    )
    .unwrap();

    assert_eq!(run.status, AnalysisParseStatusDto::Partial);
    assert_eq!(run.scanned_count, 1);
    assert!(run.artifact_count > 0);
    assert_eq!(probe.requested_limits(), vec![16 * 1024 * 1024]);
    assert_eq!(probe.full_reader_calls(), 0);
}

#[test]
fn linux_analysis_passes_each_route_limit_to_the_evidence_reader() {
    let (conn, _tmp, ds_id) = setup_case_db();
    FileRepo::new(&conn)
        .insert_batch(&[
            file_with_ds(
                "journal",
                &ds_id,
                "var/log/journal/fixture/system.journal",
                MAX_ANALYSIS_SOURCE_BYTES as u64,
            ),
            file_with_ds("auth", &ds_id, "var/log/auth.log", 64 * 1024 * 1024),
            file_with_ds("cron", &ds_id, "etc/crontab", 8 * 1024 * 1024),
        ])
        .unwrap();

    let observed = Rc::new(std::cell::RefCell::new(HashMap::new()));
    let observed_reader = Rc::clone(&observed);
    run_analysis_extraction_with_reader_limits(
        &conn,
        "case-analysis",
        L,
        &["LinuxArtifacts"],
        move |file_id, read_limit| {
            observed_reader
                .borrow_mut()
                .insert(file_id.0.clone(), read_limit);
            Ok::<Box<dyn Read>, String>(Box::new(std::io::Cursor::new(Vec::<u8>::new())))
        },
    )
    .unwrap();

    let observed = observed.borrow();
    assert_eq!(observed.get("journal"), Some(&MAX_ANALYSIS_SOURCE_BYTES));
    assert_eq!(observed.get("auth"), Some(&(16 * 1024 * 1024)));
    assert_eq!(observed.get("cron"), Some(&(4 * 1024 * 1024)));
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

    let run = run_analysis_extraction_with_reader_limits(
        &conn,
        "case-analysis",
        W,
        &[],
        |file_id, _read_limit| {
            contents
                .get(&file_id.0)
                .cloned()
                .map(|bytes| Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read>)
                .ok_or_else(|| format!("missing bytes for {}", file_id.0))
        },
    )
    .unwrap();

    assert_eq!(run.status, AnalysisParseStatusDto::Parsed);
    assert!(run.warnings.is_empty());
    assert_eq!(run.scanned_count, 6);
    assert_eq!(run.artifact_count, 16);
    assert_eq!(run.timeline_event_count, 0);

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
    assert_eq!(timeline_case_count, 0);

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
    assert!(structured.network_adapters.is_empty());

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

    let second_run = run_analysis_extraction_with_reader_limits(
        &conn,
        "case-analysis",
        W,
        &[],
        |file_id, _read_limit| {
            contents
                .get(&file_id.0)
                .cloned()
                .map(|bytes| Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read>)
                .ok_or_else(|| format!("missing bytes for {}", file_id.0))
        },
    )
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
    assert_eq!(timeline_count, 0);
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

    let run = run_analysis_extraction_with_reader_limits(
        &conn,
        "case-analysis",
        W,
        &["EventLogs"],
        |file_id, _read_limit| {
            contents
                .get(&file_id.0)
                .cloned()
                .map(|bytes| Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read>)
                .ok_or_else(|| format!("missing bytes for {}", file_id.0))
        },
    )
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
    assert_eq!(timeline_count, 0, "EVTX events must not enter Timeline");
}

#[test]
fn run_analysis_extraction_extracts_linux_artifacts_and_persists() {
    let (conn, _tmp, ds_id) = setup_case_db();
    let bash_history = "#1700000000\nls -la /home\ncat /etc/hostname\n";
    let zsh_history = ": 1700000100:0;whoami\n";
    let fish_history = "- cmd: uname -a\n  when: 1700000200\n";
    let syslog = "Jan 15 10:30:00 ubuntu sshd[1234]: Accepted publickey for alice from 192.168.1.100 port 22\n";
    let authorized_keys = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAITestKey alice@example\n";
    let passwd = "root:x:0:0:root:/root:/bin/bash\nalice:x:1000:1000:Alice:/home/alice:/bin/bash\n";
    let os_release = "PRETTY_NAME=\"CentOS Stream 9\"\nID=centos\nVERSION_ID=\"9\"\n";
    let pve_storage = "dir: local\n\tpath /var/lib/vz\n\tcontent iso,vztmpl,backup\n";
    let pve_vm_config =
        "name: prod-vm\nmemory: 4096\nnet0: virtio=AA:BB:CC:DD:EE:FF,bridge=vmbr0\n";
    let pve_lxc_config = "hostname: ct101\nrootfs: local-lvm:vm-101-disk-0,size=8G\n";
    let pve_corosync = "totem {\n  cluster_name: pve-cluster\n}\n";
    let corosync = "nodelist {\n  node {\n    name: pve01\n  }\n}\n";
    let pve_proxy_access =
        "192.0.2.10 - root@pam [15/Jan/2024:10:30:00 +0000] \"GET /api2/json/version HTTP/1.1\" 200 123\n";
    let pvedaemon = "Jan 15 10:32:00 pve01 pvedaemon[3210]: starting task UPID:pve01:00000C8A:00112233:65A52980:qmstart:100:root@pam:\n";
    let pve_task = "UPID:pve01:00000C8A:00112233:65A52980:qmstart:100:root@pam: OK\n";
    let yum_log = "Jan 15 10:33:00 Installed: curl-7.61.1-33.el8.x86_64\n";
    let nginx_config = "server {\n  listen 80;\n  server_name example.com;\n  root /var/www/html;\n  access_log /var/log/nginx/access.log;\n  error_log /var/log/nginx/error.log;\n}\n";
    let nginx_access = "192.0.2.10 - - [15/Jan/2024:10:30:45 +0000] \"GET /products?id=1%20UNION%20SELECT%20password HTTP/1.1\" 200 4532 \"-\" \"sqlmap/1.7\"\n";
    let web_shell = "<?php echo shell_exec($_GET['cmd']);\n";
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
    contents.insert("passwd".to_string(), passwd.as_bytes().to_vec());
    contents.insert("os-release".to_string(), os_release.as_bytes().to_vec());
    contents.insert("pve-storage".to_string(), pve_storage.as_bytes().to_vec());
    contents.insert("pve-vm".to_string(), pve_vm_config.as_bytes().to_vec());
    contents.insert("pve-lxc".to_string(), pve_lxc_config.as_bytes().to_vec());
    contents.insert("pve-corosync".to_string(), pve_corosync.as_bytes().to_vec());
    contents.insert("corosync".to_string(), corosync.as_bytes().to_vec());
    contents.insert(
        "pveproxy-access".to_string(),
        pve_proxy_access.as_bytes().to_vec(),
    );
    contents.insert("pvedaemon".to_string(), pvedaemon.as_bytes().to_vec());
    contents.insert("pve-task".to_string(), pve_task.as_bytes().to_vec());
    contents.insert("yum-log".to_string(), yum_log.as_bytes().to_vec());
    contents.insert("nginx-config".to_string(), nginx_config.as_bytes().to_vec());
    contents.insert("nginx-access".to_string(), nginx_access.as_bytes().to_vec());
    contents.insert("web-shell".to_string(), web_shell.as_bytes().to_vec());
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
            file_with_ds(
                "passwd",
                &ds_id,
                "Partition 2 (XFS) - cl/root/etc/passwd",
                passwd.len() as u64,
            ),
            file_with_ds(
                "os-release",
                &ds_id,
                "[P2]/cl/root/etc/os-release",
                os_release.len() as u64,
            ),
            file_with_ds(
                "pve-storage",
                &ds_id,
                "etc/pve/storage.cfg",
                pve_storage.len() as u64,
            ),
            file_with_ds(
                "pve-vm",
                &ds_id,
                "etc/pve/qemu-server/100.conf",
                pve_vm_config.len() as u64,
            ),
            file_with_ds(
                "pve-lxc",
                &ds_id,
                "etc/pve/lxc/101.conf",
                pve_lxc_config.len() as u64,
            ),
            file_with_ds(
                "pve-corosync",
                &ds_id,
                "etc/pve/corosync.conf",
                pve_corosync.len() as u64,
            ),
            file_with_ds(
                "corosync",
                &ds_id,
                "etc/corosync/corosync.conf",
                corosync.len() as u64,
            ),
            file_with_ds(
                "pveproxy-access",
                &ds_id,
                "var/log/pveproxy/access.log",
                pve_proxy_access.len() as u64,
            ),
            file_with_ds(
                "pvedaemon",
                &ds_id,
                "var/log/pvedaemon.log",
                pvedaemon.len() as u64,
            ),
            file_with_ds(
                "pve-task",
                &ds_id,
                "var/log/pve/tasks/active",
                pve_task.len() as u64,
            ),
            file_with_ds("yum-log", &ds_id, "var/log/yum.log", yum_log.len() as u64),
            file_with_ds(
                "nginx-config",
                &ds_id,
                "etc/nginx/nginx.conf",
                nginx_config.len() as u64,
            ),
            file_with_ds(
                "nginx-access",
                &ds_id,
                "var/log/nginx/access.log",
                nginx_access.len() as u64,
            ),
            file_with_ds(
                "web-shell",
                &ds_id,
                "var/www/html/shell.php",
                web_shell.len() as u64,
            ),
            file_with_ds("auth-rotated", &ds_id, "var/log/auth.log.1.gz", 96),
        ])
        .unwrap();

    let run = run_analysis_extraction_with_reader_limits(
        &conn,
        "case-analysis",
        L,
        &["LinuxArtifacts"],
        |file_id, _read_limit| {
            contents
                .get(&file_id.0)
                .cloned()
                .map(|bytes| Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read>)
                .ok_or_else(|| format!("missing bytes for {}", file_id.0))
        },
    )
    .unwrap();

    assert_eq!(run.scanned_count, 20);
    let command_section = run
        .sections
        .iter()
        .find(|section| section.key == "LinuxCommands")
        .expect("LinuxCommands section should be reported");
    assert_eq!(command_section.status, AnalysisParseStatusDto::Parsed);
    assert_eq!(command_section.scanned_count, 3);
    assert_eq!(command_section.artifact_count, 4);
    let system_config_section = run
        .sections
        .iter()
        .find(|section| section.key == "LinuxSystemConfig")
        .expect("LinuxSystemConfig section should be reported");
    assert_eq!(system_config_section.status, AnalysisParseStatusDto::Parsed);
    assert_eq!(system_config_section.scanned_count, 8);
    assert!(system_config_section.artifact_count > 0);
    let login_section = run
        .sections
        .iter()
        .find(|section| section.key == "LinuxLogin")
        .expect("LinuxLogin section should be reported");
    assert_eq!(login_section.status, AnalysisParseStatusDto::NotFound);
    assert_eq!(login_section.scanned_count, 0);
    let packages_section = run
        .sections
        .iter()
        .find(|section| section.key == "LinuxPackages")
        .expect("LinuxPackages section should be reported");
    assert_eq!(packages_section.status, AnalysisParseStatusDto::Parsed);
    assert_eq!(packages_section.scanned_count, 1);
    assert_eq!(packages_section.artifact_count, 1);
    let web_section = run
        .sections
        .iter()
        .find(|section| section.key == "LinuxWebServices")
        .expect("LinuxWebServices section should be reported");
    assert_eq!(web_section.status, AnalysisParseStatusDto::Parsed);
    assert_eq!(web_section.scanned_count, 3);
    assert!(
        web_section.artifact_count >= 5,
        "expected site, access log and findings from web inputs"
    );
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
    let web_site_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE artifact_type = 'LinuxWebSite'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let web_access_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE artifact_type = 'LinuxWebAccessLog'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let web_finding_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE artifact_type = 'LinuxWebFinding'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(web_site_count, 1);
    assert_eq!(web_access_count, 1);
    assert!(
        web_finding_count >= 3,
        "expected SQLi, scanner and web shell findings"
    );
    let package_action: String = conn
        .query_row(
            "SELECT json_extract(attrs, '$.action')
             FROM artifacts
             WHERE artifact_type = 'LinuxAptEvent'
               AND json_extract(attrs, '$.sourcePath') = 'var/log/yum.log'
             LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(package_action, "install");
    let package_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE artifact_type = 'LinuxAptEvent'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(package_count, 1);
    let ssh_line: String = conn
        .query_row(
            "SELECT json_extract(attrs, '$.line')
             FROM artifacts
             WHERE artifact_type = 'LinuxSystemConfig'
               AND json_extract(attrs, '$.sourcePath') = 'home/alice/.ssh/authorized_keys'
             LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(ssh_line, authorized_keys.trim());
    assert!(
        sudo_count > 0,
        "expected sudo artifact from gz rotated auth log"
    );
    let system_config_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE artifact_type = 'LinuxSystemConfig'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        system_config_count > 0,
        "expected LinuxSystemConfig artifacts"
    );
    let root_uid: i64 = conn
        .query_row(
            "SELECT json_extract(attrs, '$.uid')
             FROM artifacts
             WHERE artifact_type = 'LinuxSystemConfig'
               AND json_extract(attrs, '$.configKind') = 'passwdAccount'
               AND json_extract(attrs, '$.username') = 'root'
             LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(root_uid, 0);
    let pretty_name: String = conn
        .query_row(
            "SELECT json_extract(attrs, '$.prettyName')
             FROM artifacts
             WHERE artifact_type = 'LinuxSystemConfig'
               AND json_extract(attrs, '$.configKind') = 'osRelease'
             LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(pretty_name, "CentOS Stream 9");
    let pve_config_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts
             WHERE artifact_type = 'LinuxSystemConfig'
               AND json_extract(attrs, '$.configKind') = 'pveConfig'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        pve_config_count > 0,
        "expected PVE config artifacts from Proxmox paths"
    );
    let pve_vm_memory: String = conn
        .query_row(
            "SELECT json_extract(attrs, '$.value')
             FROM artifacts
             WHERE artifact_type = 'LinuxSystemConfig'
               AND json_extract(attrs, '$.pveConfigType') = 'pveQemuConfig'
               AND json_extract(attrs, '$.key') = 'memory'
             LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(pve_vm_memory, "4096");
    let pve_log_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts
             WHERE artifact_type = 'LinuxJournal'
               AND json_extract(attrs, '$.logKind') = 'pve'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        pve_log_count > 0,
        "expected PVE logs to use generic text fallback"
    );

    let summary = get_linux_artifact_summary(&conn, 0, 200).unwrap();
    assert_eq!(summary.bash_command_count, artifact_count as u64);
    assert_eq!(
        summary.total_count,
        artifact_count as u64
            + journal_count as u64
            + package_count as u64
            + sudo_count as u64
            + system_config_count as u64
            + web_site_count as u64
            + web_access_count as u64
            + web_finding_count as u64
    );
    assert_eq!(summary.web_site_count, web_site_count as u64);
    assert_eq!(summary.web_access_log_count, web_access_count as u64);
    assert_eq!(summary.web_finding_count, web_finding_count as u64);
    assert_eq!(summary.web_sites[0].server_kind, "nginx");
    assert!(summary.web_sites[0]
        .document_roots
        .contains(&"/var/www/html".to_string()));
    assert_eq!(summary.web_access_logs[0].client_ip, "192.0.2.10");
    assert!(summary
        .web_findings
        .iter()
        .any(|finding| finding.finding_kind == "webShellCandidate"));
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
            file_with_ds(
                "passwd-prefixed",
                &ds_id,
                "Partition 2 (XFS) - cl/root/etc/passwd",
                10,
            ),
            file_with_ds(
                "os-release-prefixed",
                &ds_id,
                "[P2]/cl/root/etc/os-release",
                10,
            ),
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
            file_with_ds("pve-storage", &ds_id, "etc/pve/storage.cfg", 10),
            file_with_ds("pve-qemu", &ds_id, "etc/pve/qemu-server/100.conf", 10),
            file_with_ds("pve-lxc", &ds_id, "etc/pve/lxc/101.conf", 10),
            file_with_ds("pve-corosync", &ds_id, "etc/pve/corosync.conf", 10),
            file_with_ds("corosync", &ds_id, "etc/corosync/corosync.conf", 10),
            file_with_ds("pveproxy", &ds_id, "var/log/pveproxy/access.log", 10),
            file_with_ds("pvedaemon", &ds_id, "var/log/pvedaemon.log", 10),
            file_with_ds("pve-task", &ds_id, "var/log/pve/tasks/active", 10),
        ])
        .unwrap();

    let candidates = discover_evidence_candidates(&conn).unwrap();
    let linux = candidates.get("LinuxArtifacts").unwrap();

    assert_eq!(linux.len(), 18);
    assert!(linux
        .iter()
        .any(|item| item.path.ends_with("cl/root/etc/passwd")));
    assert!(linux
        .iter()
        .any(|item| item.path.ends_with("cl/root/etc/os-release")));
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
    assert!(linux
        .iter()
        .any(|item| item.path.ends_with("etc/pve/storage.cfg")));
    assert!(linux
        .iter()
        .any(|item| item.path.ends_with("etc/pve/qemu-server/100.conf")));
    assert!(linux
        .iter()
        .any(|item| item.path.ends_with("etc/pve/lxc/101.conf")));
    assert!(linux
        .iter()
        .any(|item| item.path.ends_with("etc/pve/corosync.conf")));
    assert!(linux
        .iter()
        .any(|item| item.path.ends_with("etc/corosync/corosync.conf")));
    assert!(linux
        .iter()
        .any(|item| item.path.ends_with("var/log/pveproxy/access.log")));
    assert!(linux
        .iter()
        .any(|item| item.path.ends_with("var/log/pvedaemon.log")));
    assert!(linux
        .iter()
        .any(|item| item.path.ends_with("var/log/pve/tasks/active")));
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
                16 * 1024 * 1024 + 1,
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
        .any(|warning| warning.contains("16777216 bytes (per-source cap)")));
    assert!(summary.warnings.iter().all(|warning| {
        !warning.contains("do not yet have a structured parser")
            && !warning.contains("do not yet have a structured fallback")
    }));
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

    let summary = get_evidence_classification_summary(&conn, W).unwrap();
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

    let summary = get_evidence_classification_summary(&conn, W).unwrap();
    let program = summary
        .categories
        .iter()
        .find(|category| category.category == "ProgramExecution")
        .unwrap();
    assert_eq!(program.status, AnalysisParseStatusDto::Parsed);
    assert_eq!(program.artifact_count, 1);
    assert_eq!(program.sources[0].status, AnalysisParseStatusDto::Parsed);
}
