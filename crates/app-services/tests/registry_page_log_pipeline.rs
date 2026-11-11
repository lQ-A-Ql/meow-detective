#[path = "../../artifacts-windows/tests/support/browser_hive.rs"]
mod hive;
#[path = "../../artifacts-windows/tests/support/registry_logs.rs"]
mod logs;

use app_services::analysis_service::run_analysis_extraction;
use domain::{DataSourceId, DataSourcePlatform, EntryType, FileEntry, FileEntryId};
use persistence_source::repositories::file_repo::FileRepo;
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
    let artifacts = app_services::artifact_service::get_artifact_rows_from_db(conn, None).unwrap();
    artifacts
        .into_iter()
        .find(|artifact| {
            artifact.artifact_type == "RegistryValue"
                && artifact.attrs.get("field").and_then(|value| value.as_str())
                    == Some("productName")
        })
        .unwrap()
        .attrs["data"]
        .as_str()
        .unwrap()
        .to_string()
}

fn publish_source(conn: &Connection, source_id: &DataSourceId) {
    let store = persistence_source::detail::open_store_for_write(conn)
        .unwrap()
        .unwrap();
    persistence_fjall::replace_source_generation(&store, &source_id.0, "fixture", &"a".repeat(64))
        .unwrap();
    let key = persistence_fjall::encode_key(
        "projection",
        &[persistence_fjall::KeyPart::Text(source_id.0.clone())],
    )
    .unwrap();
    let marker = persistence_fjall::VersionedEnvelope::new(
        "source_projection_complete",
        1,
        format!("projection:{}", source_id.0),
        &source_id.0,
        1,
        source_id.0.as_bytes().to_vec(),
    );
    store
        .write_envelope(
            persistence_fjall::KeyspaceName::Meta,
            key,
            &marker,
            persistence_fjall::PersistMode::SyncAll,
        )
        .unwrap();
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
    let source_root = tempfile::tempdir().unwrap();
    let source_id = DataSourceId("source-1".into());
    let conn = app_services::source_db::open_source_db(source_root.path(), &source_id).unwrap();
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
    publish_source(&conn, &source_id);
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
    let mut updated_log = entry(
        workspace.path(),
        "log",
        "[P1]/Windows/System32/config/SOFTWARE.LOG1",
    );
    updated_log.hash_sha256 = Some(infrastructure::hashing::sha256_bytes(&updated));
    FileRepo::new(&conn).insert_batch(&[updated_log]).unwrap();
    publish_source(&conn, &source_id);
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
    let count = app_services::artifact_service::get_artifact_rows_from_db(&conn, None)
        .unwrap()
        .into_iter()
        .filter(|artifact| {
            artifact.artifact_type == "RegistryValue"
                && artifact.attrs.get("field").and_then(|value| value.as_str())
                    == Some("productName")
        })
        .count() as u64;
    assert_eq!(count, 1);
}
