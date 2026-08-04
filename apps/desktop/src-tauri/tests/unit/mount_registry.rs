use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use domain::{CaseId, DataSourceId};
use evidence_mount::{MountAccess, MountPath, MountReadPolicy};

use super::{MountRegistry, MountRegistryError};
use crate::mount_backend::MountBackendError;

const CASE_ROOT_ENV: &str = "FORENSICS_MOUNT_CASE_ROOT";
const DATA_SOURCE_ID_ENV: &str = "FORENSICS_MOUNT_DATA_SOURCE_ID";
const PARTITION_INDEX_ENV: &str = "FORENSICS_MOUNT_PARTITION_INDEX";
const MOUNT_POINT_ENV: &str = "FORENSICS_MOUNT_POINT";

#[test]
fn invalid_mount_points_are_validation_errors() {
    use transport::ServiceErrorCategory;

    let error = MountRegistryError::Backend(MountBackendError::InvalidMountPoint(
        "drive is occupied".to_string(),
    ));
    assert!(matches!(
        error.category(),
        transport::ErrorCategory::Validation
    ));
}

#[test]
#[ignore = "requires an imported E01 case and the Dokan runtime"]
fn real_e01_mount_reads_through_a_read_only_drive_and_releases_it() {
    let case_root = PathBuf::from(required_env(CASE_ROOT_ENV));
    let data_source_id = DataSourceId(required_env(DATA_SOURCE_ID_ENV));
    let partition_index = required_env(PARTITION_INDEX_ENV)
        .parse::<usize>()
        .expect("mount partition index must be an unsigned integer");
    let requested_mount_point = std::env::var(MOUNT_POINT_ENV).ok();
    let case_conn = persistence_sqlite::connection::open_existing(&case_root.join("app.db"))
        .expect("case control database must open");
    let case_id = CaseId(
        case_conn
            .query_row("SELECT id FROM cases LIMIT 1", [], |row| row.get(0))
            .expect("case row must exist"),
    );
    app_services::source_db::migrate_ready_source_databases(&case_conn, &case_root, &case_id)
        .expect("case-open source database migrations must succeed");
    let source_db = source_db_path(&case_conn, &case_root, &data_source_id);
    let expected_root_entries =
        mountable_root_child_count(&source_db, &data_source_id, partition_index);
    let relative_path = readable_file_path(&source_db, &data_source_id, partition_index);
    let mount_path = MountPath::parse(&relative_path).expect("catalog path must be mountable");
    let session = app_services::mount_service::prepare_mount_session(
        &case_conn,
        &case_root,
        &case_id,
        &data_source_id,
        partition_index,
        MountReadPolicy::default(),
    )
    .expect("real E01 mount session must open");
    let handle_id = session
        .open(&mount_path, MountAccess::ReadOnly)
        .expect("direct session file must open");
    let expected = session
        .read_at(handle_id, 0, 64 * 1024)
        .expect("direct session file must read");
    session.close(handle_id).expect("direct handle must close");

    let registry = MountRegistry::default();
    let status = registry
        .mount_session(session, requested_mount_point.as_deref(), &case_id.0)
        .expect("Dokan mount must start");
    let mounted_file = mounted_path(&status.target.mount_point, &relative_path);
    let mut actual = Vec::new();
    std::fs::File::open(&mounted_file)
        .expect("mounted file must open through the host filesystem")
        .take(expected.len() as u64)
        .read_to_end(&mut actual)
        .expect("mounted file must read through the host filesystem");
    assert_eq!(actual, expected);
    let enumeration_started = Instant::now();
    let actual_root_entries = std::fs::read_dir(format!("{}\\", status.target.mount_point))
        .expect("mounted root must enumerate")
        .try_fold(0usize, |count, entry| entry.map(|_| count + 1))
        .expect("mounted root entries must be readable");
    let enumeration_elapsed = enumeration_started.elapsed();
    eprintln!("mounted_root_entries={actual_root_entries} elapsed={enumeration_elapsed:?}");
    assert_eq!(actual_root_entries, expected_root_entries);
    if expected_root_entries >= 10_000 {
        assert!(
            enumeration_elapsed < Duration::from_secs(5),
            "large mounted directory exceeded the 5 second regression limit: {enumeration_elapsed:?}"
        );
    }
    assert!(std::fs::OpenOptions::new()
        .write(true)
        .open(&mounted_file)
        .is_err());

    registry
        .unmount(&status.target.mount_id)
        .expect("Dokan mount must stop");
    wait_until_released(&status.target.mount_point);
}

#[test]
#[ignore = "requires an imported E01 case and the Dokan runtime"]
fn real_e01_mount_streams_a_large_file_with_bounded_latency() {
    let case_root = PathBuf::from(required_env(CASE_ROOT_ENV));
    let data_source_id = DataSourceId(required_env(DATA_SOURCE_ID_ENV));
    let partition_index = required_env(PARTITION_INDEX_ENV)
        .parse::<usize>()
        .expect("mount partition index must be an unsigned integer");
    let requested_mount_point = std::env::var(MOUNT_POINT_ENV).ok();
    let case_conn = persistence_sqlite::connection::open_existing(&case_root.join("app.db"))
        .expect("case control database must open");
    let case_id = CaseId(
        case_conn
            .query_row("SELECT id FROM cases LIMIT 1", [], |row| row.get(0))
            .expect("case row must exist"),
    );
    app_services::source_db::migrate_ready_source_databases(&case_conn, &case_root, &case_id)
        .expect("case-open source database migrations must succeed");
    let source_db = source_db_path(&case_conn, &case_root, &data_source_id);
    let session = app_services::mount_service::prepare_mount_session(
        &case_conn,
        &case_root,
        &case_id,
        &data_source_id,
        partition_index,
        MountReadPolicy::default(),
    )
    .expect("real E01 mount session must open");
    let (relative_path, expected_size) =
        readable_large_file_path(&source_db, &data_source_id, partition_index, &session);

    let registry = MountRegistry::default();
    let status = registry
        .mount_session(session, requested_mount_point.as_deref(), &case_id.0)
        .expect("Dokan mount must start");
    let mounted_file = mounted_path(&status.target.mount_point, &relative_path);
    let started = Instant::now();
    let copied = io::copy(
        &mut std::fs::File::open(&mounted_file).expect("mounted large file must open"),
        &mut io::sink(),
    )
    .expect("mounted large file must stream");
    let elapsed = started.elapsed();
    eprintln!("mounted_read_bytes={copied} elapsed={elapsed:?}");
    assert_eq!(copied, expected_size);
    assert!(
        elapsed < Duration::from_secs(30),
        "mounted {copied}-byte file exceeded the 30 second regression limit: {elapsed:?}"
    );

    registry
        .unmount(&status.target.mount_id)
        .expect("Dokan mount must stop");
    wait_until_released(&status.target.mount_point);
}

fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set for the real mount test"))
}

fn source_db_path(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> PathBuf {
    let relative: String = case_conn
        .query_row(
            "SELECT source_db_rel_path FROM data_sources WHERE id = ?1",
            [&data_source_id.0],
            |row| row.get(0),
        )
        .expect("source database path must exist");
    case_root.join(relative)
}

fn readable_file_path(
    source_db: &Path,
    data_source_id: &DataSourceId,
    partition_index: usize,
) -> String {
    let connection = persistence_sqlite::connection::open_existing_source_read_only(source_db)
        .expect("source database must open read-only");
    let mut statement = connection
        .prepare(
            "SELECT path FROM file_entries
             WHERE data_source_id = ?1 AND partition_index = ?2
               AND LOWER(entry_type) != 'directory' AND encrypted = 0 AND deleted = 0
               AND size BETWEEN 1 AND 1048576
             ORDER BY size ASC, path ASC LIMIT 256",
        )
        .expect("mount sample query must prepare");
    let path = statement
        .query_map(
            rusqlite::params![data_source_id.0, partition_index as u64],
            |row| row.get::<_, String>(0),
        )
        .expect("mount sample query must run")
        .filter_map(Result::ok)
        .map(|path| strip_partition_prefix(&path, partition_index))
        .find(|path| MountPath::parse(path).is_ok())
        .expect("partition must contain a small Windows-representable file");
    path
}

fn mountable_root_child_count(
    source_db: &Path,
    data_source_id: &DataSourceId,
    partition_index: usize,
) -> usize {
    let connection = persistence_sqlite::connection::open_existing_source_read_only(source_db)
        .expect("source database must open read-only");
    let root_id: String = connection
        .query_row(
            "SELECT id FROM file_entries
             WHERE parent_id IS NULL AND data_source_id = ?1 AND partition_index = ?2
               AND deleted = 0
             ORDER BY id ASC LIMIT 1",
            rusqlite::params![data_source_id.0, partition_index as u64],
            |row| row.get(0),
        )
        .expect("partition root must exist");
    let mut statement = connection
        .prepare(
            "SELECT name FROM file_entries
             WHERE parent_id = ?1 AND data_source_id = ?2 AND partition_index = ?3
               AND deleted = 0",
        )
        .expect("root child count query must prepare");
    statement
        .query_map(
            rusqlite::params![root_id, data_source_id.0, partition_index as u64],
            |row| row.get::<_, String>(0),
        )
        .expect("root child count query must run")
        .filter_map(Result::ok)
        .filter(|name| MountPath::parse(name).is_ok())
        .count()
}

fn readable_large_file_path(
    source_db: &Path,
    data_source_id: &DataSourceId,
    partition_index: usize,
    session: &evidence_mount::MountSession,
) -> (String, u64) {
    let connection = persistence_sqlite::connection::open_existing_source_read_only(source_db)
        .expect("source database must open read-only");
    let mut statement = connection
        .prepare(
            "SELECT path, size FROM file_entries
             WHERE data_source_id = ?1 AND partition_index = ?2
               AND LOWER(entry_type) != 'directory' AND encrypted = 0 AND deleted = 0
               AND size BETWEEN 8388608 AND 67108864
             ORDER BY size ASC, path ASC LIMIT 256",
        )
        .expect("large mount sample query must prepare");
    let candidates = statement
        .query_map(
            rusqlite::params![data_source_id.0, partition_index as u64],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)),
        )
        .expect("large mount sample query must run");
    for candidate in candidates.filter_map(Result::ok) {
        let path = strip_partition_prefix(&candidate.0, partition_index);
        let Ok(mount_path) = MountPath::parse(&path) else {
            continue;
        };
        let Ok(handle_id) = session.open(&mount_path, MountAccess::ReadOnly) else {
            continue;
        };
        let readable = session
            .read_at(handle_id, 0, 4096)
            .is_ok_and(|bytes| !bytes.is_empty());
        let _ = session.close(handle_id);
        if readable {
            return (path, candidate.1);
        }
    }
    panic!("partition must contain a readable 8-64 MiB Windows-representable file")
}

fn strip_partition_prefix(path: &str, partition_index: usize) -> String {
    path.trim_start_matches('/')
        .strip_prefix(&format!("[P{partition_index}]/"))
        .unwrap_or(path.trim_start_matches('/'))
        .to_string()
}

fn mounted_path(mount_point: &str, relative_path: &str) -> PathBuf {
    PathBuf::from(format!(
        "{}\\{}",
        mount_point,
        relative_path.replace('/', "\\")
    ))
}

fn wait_until_released(mount_point: &str) {
    let root = format!("{mount_point}\\");
    for _ in 0..50 {
        if !Path::new(&root).exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("mount point was not released: {mount_point}");
}
