use app_services::analysis_service::run_analysis_extraction;
use domain::{DataSourceId, DataSourcePlatform, EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::artifact_repo::ArtifactRepo;
use rusqlite::Connection;
use std::{cell::Cell, io::Read, path::Path};

#[path = "support/native_analysis_fixture.rs"]
mod native_analysis_fixture;

use native_analysis_fixture::NativeAnalysisFixture;

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

fn ids(connection: &NativeAnalysisFixture) -> Vec<String> {
    let mut artifacts = ArtifactRepo::new(connection)
        .list_by_family(Some("BrowserHistory"))
        .unwrap();
    artifacts.sort_by(|left, right| left.id.0.cmp(&right.id.0));
    artifacts
        .into_iter()
        .map(|artifact| artifact.id.0)
        .collect()
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
    let connection = NativeAnalysisFixture::new(SOURCE, "case-browser");
    connection.insert_file(entry(workspace.path(), "history", HISTORY.into()));
    connection.insert_file(entry(workspace.path(), "wal", format!("{HISTORY}-wal")));
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
    let artifacts = ArtifactRepo::new(&connection)
        .list_by_family(Some("BrowserHistory"))
        .unwrap();
    let raw = artifacts
        .iter()
        .find(|artifact| {
            artifact
                .attrs
                .get("visitId")
                .is_some_and(|value| value.to_string().trim_matches('"') == "2107")
        })
        .and_then(|artifact| artifact.attrs.get("rawTimestamp"))
        .expect("browser artifact raw timestamp")
        .to_string()
        .trim_matches('"')
        .to_string();
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
    connection.update_file_metadata(
        "wal",
        updated.len() as u64,
        infrastructure::hashing::sha256_bytes(&updated),
    );
    connection
        .execute_batch(
            "CREATE TRIGGER stop_browser BEFORE INSERT ON forensic_fingerprints
             BEGIN SELECT RAISE(ABORT,'native output commit failure'); END;",
        )
        .unwrap();
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
