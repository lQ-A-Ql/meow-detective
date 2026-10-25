use app_services::analysis_service::run_analysis_extraction;
use domain::{DataSourceId, DataSourcePlatform, EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::{params, Connection};
use std::{cell::Cell, io::Read, path::Path};

const SOURCE: &str = "browser-source";
const HISTORY: &str = "[P1]/Users/A/AppData/Local/Google/Chrome/User Data/Default/History";

fn entry(root: &Path, id: &str, path: String) -> FileEntry {
    let bytes = std::fs::read(root.join(id)).unwrap();
    FileEntry {
        id: FileEntryId(id.into()),
        parent_id: None,
        data_source_id: DataSourceId(SOURCE.into()),
        name: path.rsplit('/').next().unwrap().into(),
        path,
        entry_type: EntryType::File,
        size: Some(bytes.len() as u64),
        ext: None,
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        read_only: true,
        archive: false,
        unix_mode: None,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: Some(infrastructure::hashing::sha256_bytes(&bytes)),
    }
}

fn history(connection: &Connection, first: i64, last: i64) {
    let transaction = connection.unchecked_transaction().unwrap();
    for id in first..=last {
        transaction
            .execute("INSERT INTO visits VALUES(?1,1,13300000000000001+?1)", [id])
            .unwrap();
    }
    transaction.commit().unwrap();
}

fn ids(connection: &Connection) -> Vec<String> {
    connection
        .prepare("SELECT id FROM artifacts WHERE artifact_type='BrowserHistory' ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

#[test]
fn wal_discovery_full_streaming_checkpoint_reuse_and_failed_replacement_are_atomic() {
    let workspace = tempfile::tempdir().unwrap();
    let writer = Connection::open(workspace.path().join("input.sqlite")).unwrap();
    writer.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;
        CREATE TABLE urls(id INTEGER PRIMARY KEY,url TEXT,title TEXT,visit_count INTEGER,last_visit_time INTEGER);
        CREATE TABLE visits(id INTEGER PRIMARY KEY,url INTEGER,visit_time INTEGER);
        INSERT INTO urls VALUES(1,'https://example.test/','Page',2107,13300000000000001);").unwrap();
    history(&writer, 1, 1);
    writer
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    history(&writer, 2, 2107);
    let main = std::fs::read(workspace.path().join("input.sqlite")).unwrap();
    let wal = std::fs::read(workspace.path().join("input.sqlite-wal")).unwrap();
    std::fs::write(workspace.path().join("history"), &main).unwrap();
    std::fs::write(workspace.path().join("wal"), &wal).unwrap();
    let connection = Connection::open(workspace.path().join("source.db")).unwrap();
    persistence_sqlite::runner::run_source_all(&connection).unwrap();
    connection
        .execute(
            "INSERT INTO data_sources(id,case_id,name,kind,source_path,imported_at)
        VALUES(?1,'case-browser','Browser','directory',?2,'2026-10-26T00:00:00Z')",
            params![SOURCE, workspace.path().to_str().unwrap()],
        )
        .unwrap();
    FileRepo::new(&connection)
        .insert_batch(&[
            entry(workspace.path(), "history", HISTORY.into()),
            entry(workspace.path(), "wal", format!("{HISTORY}-wal")),
        ])
        .unwrap();
    let reads = Cell::new(0);
    let mut reader = |id: &FileEntryId| -> Result<Box<dyn Read>, std::io::Error> {
        reads.set(reads.get() + 1);
        Ok(Box::new(std::fs::File::open(workspace.path().join(&id.0))?))
    };
    let first = run_analysis_extraction(
        &connection,
        "case-browser",
        DataSourcePlatform::Windows,
        &["BrowserHistory"],
        &mut reader,
    )
    .unwrap();
    assert_eq!(first.artifact_count, 2107, "{:?}", first.warnings);
    assert_eq!(ids(&connection).len(), 2107);
    let raw: String = connection
        .query_row(
            "SELECT json_extract(attrs,'$.rawTimestamp') FROM artifacts
        WHERE json_extract(attrs,'$.visitId')='2107'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(raw, (13300000000000001i64 + 2107).to_string());
    assert_eq!(reads.get(), 2);
    let replay = run_analysis_extraction(
        &connection,
        "case-browser",
        DataSourcePlatform::Windows,
        &["BrowserHistory"],
        &mut reader,
    )
    .unwrap();
    assert!(replay.checkpoint_hit_count > 0);
    assert_eq!(reads.get(), 2);
    let previous = ids(&connection);
    history(&writer, 2108, 2200);
    let updated = std::fs::read(workspace.path().join("input.sqlite-wal")).unwrap();
    std::fs::write(workspace.path().join("wal"), &updated).unwrap();
    connection
        .execute(
            "UPDATE file_entries SET size=?1,hash_sha256=?2 WHERE id='wal'",
            params![
                updated.len() as u64,
                infrastructure::hashing::sha256_bytes(&updated)
            ],
        )
        .unwrap();
    connection.execute_batch("CREATE TRIGGER stop_browser BEFORE INSERT ON artifacts
        WHEN json_extract(NEW.attrs,'$.visitId')='700' BEGIN SELECT RAISE(ABORT,'write failure'); END;").unwrap();
    assert!(run_analysis_extraction(
        &connection,
        "case-browser",
        DataSourcePlatform::Windows,
        &["BrowserHistory"],
        &mut reader
    )
    .is_err());
    assert_eq!(ids(&connection), previous);
    connection
        .execute_batch("DROP TRIGGER stop_browser")
        .unwrap();
    let refreshed = run_analysis_extraction(
        &connection,
        "case-browser",
        DataSourcePlatform::Windows,
        &["BrowserHistory"],
        &mut reader,
    )
    .unwrap();
    assert_eq!(refreshed.artifact_count, 2200, "{:?}", refreshed.warnings);
    assert_eq!(ids(&connection).len(), 2200);
    assert_eq!(
        std::fs::read(workspace.path().join("history")).unwrap(),
        main
    );
    assert_eq!(
        std::fs::read(workspace.path().join("wal")).unwrap(),
        updated
    );
}
