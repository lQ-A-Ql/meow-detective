#[path = "../../artifacts-windows/tests/support/browser_hive.rs"]
mod hive;
#[path = "../../artifacts-windows/tests/support/registry_logs.rs"]
mod logs;

use app_services::analysis_service::run_analysis_extraction;
use domain::{DataSourceId, DataSourcePlatform, EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::{params, Connection};
use std::{cell::Cell, fs::File, io::Read, path::Path};

fn fixture() -> (Vec<u8>, u32) {
    let mut hive = hive::Hive::new();
    let raw: Vec<_> = "OLD-OS\0"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let value = hive.value("ProductName", 1, &raw, false);
    let version = hive.key("CurrentVersion", &[], &[value]);
    let windows = hive.key("Windows NT", &[version], &[]);
    let microsoft = hive.key("Microsoft", &[windows], &[]);
    let root = hive.key("SOFTWARE", &[microsoft], &[]);
    let mut bytes = hive.finish(root);
    logs::seal(&mut bytes, 11, 10);
    (bytes, value)
}

fn log(base: &[u8], value: u32, text: &str, old: bool) -> Vec<u8> {
    let mut pages = base[4096..].to_vec();
    let offset = u32::from_le_bytes(
        pages[value as usize + 12..value as usize + 16]
            .try_into()
            .unwrap(),
    ) as usize
        + 4;
    let raw: Vec<_> = text
        .encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect();
    pages[offset..offset + raw.len()].copy_from_slice(&raw);
    if old {
        logs::old_log(base, 11, &pages)
    } else {
        logs::log(base, 10, &[(0, pages)])
    }
}

fn entry(root: &Path, id: &str, path: &str) -> FileEntry {
    let bytes = std::fs::read(root.join(id)).unwrap();
    FileEntry {
        id: FileEntryId(id.into()),
        parent_id: None,
        data_source_id: DataSourceId("source-1".into()),
        path: path.into(),
        name: path.rsplit('/').next().unwrap().into(),
        entry_type: EntryType::File,
        size: Some(bytes.len() as u64),
        ext: None,
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        read_only: false,
        archive: false,
        unix_mode: None,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: Some(infrastructure::hashing::sha256_bytes(&bytes)),
    }
}

fn product(conn: &Connection) -> String {
    let raw: String = conn.query_row(
        "SELECT attrs FROM artifacts WHERE artifact_type='RegistryValue' AND json_extract(attrs,'$.field')='productName'",
        [], |row| row.get(0),
    ).unwrap();
    serde_json::from_str::<serde_json::Value>(&raw).unwrap()["data"]
        .as_str()
        .unwrap()
        .into()
}

#[test]
fn discovery_reads_actual_companion_files_and_log_changes_expire_the_checkpoint() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/test-workspaces");
    std::fs::create_dir_all(&parent).unwrap();
    let workspace = tempfile::Builder::new()
        .prefix("registry-page-log-")
        .tempdir_in(parent)
        .unwrap();
    let (base, value) = fixture();
    std::fs::write(workspace.path().join("hive"), &base).unwrap();
    std::fs::write(
        workspace.path().join("log"),
        log(&base, value, "NEW-OS", false),
    )
    .unwrap();
    let conn = Connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_source_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at)
         VALUES ('source-1', 'case-1', 'Registry files', 'directory', ?1, '2026-10-25T00:00:00Z')",
        params![workspace.path().to_str().unwrap()],
    )
    .unwrap();
    FileRepo::new(&conn)
        .insert_batch(&[
            entry(
                workspace.path(),
                "hive",
                "[P1]/Windows/System32/config/SOFTWARE",
            ),
            entry(
                workspace.path(),
                "log",
                "[P1]/Windows/System32/config/SOFTWARE.LOG1",
            ),
        ])
        .unwrap();
    let reads = Cell::new(0);
    let mut reader = |id: &FileEntryId| -> Result<Box<dyn Read>, std::io::Error> {
        reads.set(reads.get() + 1);
        Ok(Box::new(File::open(workspace.path().join(&id.0))?))
    };
    run_analysis_extraction(
        &conn,
        "case-1",
        DataSourcePlatform::Windows,
        &["Registry"],
        &mut reader,
    )
    .unwrap();
    assert_eq!(product(&conn), "NEW-OS");
    assert_eq!(reads.get(), 2);
    run_analysis_extraction(
        &conn,
        "case-1",
        DataSourcePlatform::Windows,
        &["Registry"],
        &mut reader,
    )
    .unwrap();
    assert_eq!(reads.get(), 2);
    let updated = log(&base, value, "ALT-OS", true);
    std::fs::write(workspace.path().join("log"), &updated).unwrap();
    conn.execute(
        "UPDATE file_entries SET hash_sha256=?1 WHERE id='log'",
        params![infrastructure::hashing::sha256_bytes(&updated)],
    )
    .unwrap();
    run_analysis_extraction(
        &conn,
        "case-1",
        DataSourcePlatform::Windows,
        &["Registry"],
        &mut reader,
    )
    .unwrap();
    assert_eq!(product(&conn), "ALT-OS");
    assert_eq!(reads.get(), 4);
    let count: u64 = conn.query_row("SELECT COUNT(*) FROM artifacts WHERE artifact_type='RegistryValue' AND json_extract(attrs,'$.field')='productName'", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 1);
}
