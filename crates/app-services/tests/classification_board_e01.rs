//! End-to-end classification board verification on the liuyang sample:
//! hives -> 注册表配置单元, evtx -> 日志文档, SQLite -> SQLite 数据库,
//! chrome.exe -> Windows 可执行, all decided from header bytes.

use app_services::analysis_service::build_file_classification_board;
use app_services::datasource_service::detect_image_filesystem;
use domain::FileEntryId;
use evidence_core::{EvidenceReader, FileSystemReader};
use fs_ntfs::NtfsReader;
use image_e01::E01Reader;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

const ROWS: &[&str] = &[
    "Windows/System32/config/SYSTEM",
    "Windows/System32/config/SAM",
    "Windows/System32/winevt/Logs/System.evtx",
    "Users/刘洋/AppData/Local/Google/Chrome/User Data/Default/Network/Cookies",
    "Users/刘洋/AppData/Local/Google/Chrome/User Data/Default/Login Data",
    "Users/刘洋/AppData/Local/Google/Chrome/User Data/Local State",
    "Program Files/Google/Chrome/Application/chrome.exe",
];

fn open_ntfs(fixture: &Path) -> NtfsReader {
    let mut image = E01Reader::open(fixture).expect("open E01");
    let probe = detect_image_filesystem(&mut image).expect("probe E01");
    let ntfs = probe
        .candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.kind,
                app_services::datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .expect("NTFS candidate");
    let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(fixture).expect("reopen E01"));
    NtfsReader::open(boxed, ntfs.offset).expect("open NTFS")
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn liuyang_classification_board_uses_magic_bytes() {
    let fixture = std::env::var_os("FORENSICS_LIUYANG_E01_FIXTURE")
        .map(PathBuf::from)
        .expect("set FORENSICS_LIUYANG_E01_FIXTURE");
    let fs = open_ntfs(&fixture);

    let mut paths = HashMap::new();
    let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
    conn.execute_batch(
        "CREATE TABLE file_entries(
            id TEXT PRIMARY KEY, parent_id TEXT, data_source_id TEXT, path TEXT, name TEXT,
            entry_type TEXT, size INTEGER, ext TEXT, deleted INTEGER, hidden INTEGER,
            system INTEGER, created_at TEXT, modified_at TEXT, accessed_at TEXT,
            changed_at TEXT, hash_sha256 TEXT
        );",
    )
    .expect("create file_entries");
    for (index, path) in ROWS.iter().enumerate() {
        let name = path.rsplit('/').next().unwrap();
        let ext = name.rsplit('.').next().filter(|e| *e != name).unwrap_or("");
        let id = format!("file:{index}");
        paths.insert(id.clone(), *path);
        conn.execute(
            "INSERT INTO file_entries VALUES (?1, NULL, 'ds-1', ?2, ?3, 'file', 4096, ?4, 0, 0, 0, '', '', '', '', '')",
            rusqlite::params![id, format!("[P3]/{path}"), name, ext],
        )
        .expect("insert file row");
    }

    let mut reader = |file_id: &FileEntryId| -> Result<Vec<u8>, String> {
        let path = paths
            .get(&file_id.0)
            .ok_or_else(|| format!("unknown file id {}", file_id.0))?;
        let mut file = fs
            .open_file(path)
            .map_err(|error| format!("open {path}: {error}"))?;
        let mut header = vec![0u8; 16];
        let mut read = 0usize;
        while read < header.len() {
            match file.read(&mut header[read..]) {
                Ok(0) => break,
                Ok(n) => read += n,
                Err(error) => return Err(format!("read {path}: {error}")),
            }
        }
        header.truncate(read);
        Ok(header)
    };

    let board = build_file_classification_board(&conn, 300, &mut reader).expect("build board");
    eprintln!(
        "board: files={} magic={} groups={}",
        board.total_files,
        board.magic_classified_count,
        board.groups.len()
    );

    for group in &board.groups {
        for sub in &group.subcategories {
            for file in &sub.files {
                eprintln!(
                    "  [{} / {}] {} type={:?} source={}",
                    group.category,
                    sub.name,
                    file.name,
                    file.magic_type,
                    file.classification_source
                );
            }
        }
    }
    assert_eq!(board.total_files, ROWS.len() as u64);
    // All rows except the magic-less "Local State" JSON classify from headers.
    assert_eq!(board.magic_classified_count, ROWS.len() as u64 - 1);
    assert_eq!(board.metadata_classified_count, 1);

    let find_row = |subcategory: &str, needle: &str| {
        board
            .groups
            .iter()
            .flat_map(|group| &group.subcategories)
            .filter(|sub| sub.name == subcategory)
            .flat_map(|sub| &sub.files)
            .find(|file| file.path.contains(needle))
            .map(|file| (file.magic_type.clone(), file.classification_source.clone()))
    };

    assert_eq!(
        find_row("注册表配置单元", "config/SYSTEM"),
        Some((Some("REG".to_string()), "magic".to_string()))
    );
    assert_eq!(
        find_row("日志文档", "System.evtx"),
        Some((Some("EVTX".to_string()), "magic".to_string()))
    );
    assert_eq!(
        find_row("SQLite 数据库", "Cookies"),
        Some((Some("SQLite".to_string()), "magic".to_string()))
    );
    assert_eq!(
        find_row("Windows 可执行", "chrome.exe"),
        Some((Some("PE".to_string()), "magic".to_string()))
    );

    // The extensionless "Local State" JSON lands in 未识别 via metadata fallback.
    let other = board
        .groups
        .iter()
        .find(|group| group.category == "other")
        .expect("other group");
    assert!(other.subcategories.iter().any(|sub| sub.name == "未识别"
        && sub
            .files
            .iter()
            .any(|f| f.name == "Local State" && f.classification_source == "metadata")));
}
