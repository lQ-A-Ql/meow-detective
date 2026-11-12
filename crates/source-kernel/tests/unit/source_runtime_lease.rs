use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Weak};

use evidence_core::reader::{ReaderAdmission, ReaderAllocationScope, ReaderResourceHold};
use evidence_core::{
    EvidenceReader, EvidenceReaderFactory, RawImageReader, ReaderCapabilities, SharedReaderFactory,
};

use crate::resource_budget::{ResourcePool, Resources, MIB};
use crate::source_runtime::resources::{BudgetedReader, ReaderBudget, ResourceRoute};

use super::{invalidate_case, invalidate_source, SourcePublicationLease};

struct RawFactory {
    path: PathBuf,
    size: u64,
    route: ResourceRoute,
    reader_bytes: u64,
    metadata: Option<Arc<dyn ReaderResourceHold>>,
}

impl EvidenceReaderFactory for RawFactory {
    fn bind_reader_scope(&self, scope: Arc<dyn ReaderAllocationScope>) {
        self.route.bind(scope);
    }
    fn reader_memory_bound(&self) -> u64 {
        self.reader_bytes
    }
    fn reader_scope(&self) -> Option<Arc<dyn ReaderAllocationScope>> {
        self.route.scope()
    }
    fn open_reader(&self) -> io::Result<Box<dyn EvidenceReader>> {
        let hold = match &self.metadata {
            Some(_) => Some(ReaderAdmission::acquire(&ReaderBudget {
                bytes: self.reader_bytes,
                handles: 1,
                route: self.route.clone(),
            })?),
            None => None,
        };
        let reader = Box::new(RawImageReader::open(&self.path)?) as Box<dyn EvidenceReader>;
        match (hold, &self.metadata) {
            (Some(hold), Some(metadata)) => Ok(Box::new(BudgetedReader {
                reader,
                _hold: hold,
                _metadata: metadata.clone(),
            })),
            _ => Ok(reader),
        }
    }
    fn logical_size(&self) -> u64 {
        self.size
    }
    fn preferred_read_granularity(&self) -> usize {
        512
    }
    fn capabilities(&self) -> ReaderCapabilities {
        ReaderCapabilities::default()
    }
    fn validate_snapshot(&self) -> io::Result<()> {
        if std::fs::metadata(&self.path)?.len() == self.size {
            Ok(())
        } else {
            Err(io::Error::other(super::SourceInvalidated))
        }
    }
}

fn build_factory(path: &Path, reader_bytes: u64, reserve: bool) -> SharedReaderFactory {
    let size = std::fs::metadata(path).unwrap().len();
    Arc::new(RawFactory {
        path: path.to_path_buf(),
        size,
        route: ResourceRoute::default(),
        reader_bytes,
        metadata: reserve.then(|| Arc::new(()) as Arc<dyn ReaderResourceHold>),
    })
}

fn source_file(temp: &tempfile::TempDir) -> PathBuf {
    let path = temp.path().join("source.raw");
    std::fs::write(&path, [0; 512]).unwrap();
    path
}

fn lease(case: &str, path: &Path) -> SourcePublicationLease {
    SourcePublicationLease::acquire(case, "source", build_factory(path, MIB, false))
}

#[test]
fn superseded_lease_cannot_open_or_commit() {
    let temp = tempfile::tempdir().unwrap();
    let path = source_file(&temp);
    let old = lease("case", &path);
    let _new = lease("case", &path);
    let c = rusqlite::Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE output(id INTEGER PRIMARY KEY)")
        .unwrap();
    let fence = old.fence_connection(&c);
    c.execute_batch("BEGIN; INSERT INTO output VALUES(1)")
        .unwrap();
    assert!(old.validate().is_err());
    assert!(c.execute_batch("COMMIT").is_err());
    assert!(c.is_autocommit());
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM output", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        0
    );
    drop(fence);
}

#[test]
fn physical_change_between_begin_and_commit_rolls_back_output() {
    let temp = tempfile::tempdir().unwrap();
    let path = source_file(&temp);
    let lease = lease("case", &path);
    let c = rusqlite::Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE output(id INTEGER PRIMARY KEY)")
        .unwrap();
    let _fence = lease.fence_connection(&c);
    c.execute_batch("BEGIN; INSERT INTO output VALUES(1)")
        .unwrap();
    std::fs::write(&path, [0; 1024]).unwrap();
    assert!(c.execute_batch("COMMIT").is_err());
    assert!(c.is_autocommit());
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM output", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn invalidation_inside_sql_callback_never_waits_for_the_current_publisher() {
    let temp = tempfile::tempdir().unwrap();
    let path = source_file(&temp);
    let lease = lease("case", &path);
    let c = rusqlite::Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE output(id INTEGER PRIMARY KEY)")
        .unwrap();
    c.create_scalar_function(
        "retire_source",
        0,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8,
        move |_| {
            invalidate_source("case", "source");
            Ok(0)
        },
    )
    .unwrap();
    c.execute_batch(
        "CREATE TRIGGER retire BEFORE INSERT ON output BEGIN SELECT retire_source(); END",
    )
    .unwrap();
    let _fence = lease.fence_connection(&c);
    assert!(c.execute_batch("INSERT INTO output VALUES(1)").is_err());
    assert!(lease.validate().is_err());
}

#[test]
fn invalidation_scopes_to_one_case_and_prunes_dropped_epochs() {
    let temp = tempfile::tempdir().unwrap();
    let path = source_file(&temp);
    let first = lease("case-a", &path);
    let second = lease("case-b", &path);
    invalidate_case("case-a");
    assert!(first.validate().is_err());
    assert!(second.validate().is_ok());
    drop(first);
    let epochs = super::EPOCHS.lock().unwrap_or_else(|p| p.into_inner());
    let key = ("case-a".to_string(), "source".to_string());
    assert!(epochs.get(&key).and_then(Weak::upgrade).is_none());
}

#[test]
fn superseded_failure_cannot_overwrite_newer_ready_after_new_lease_drops() {
    let temp = tempfile::tempdir().unwrap();
    let path = source_file(&temp);
    let old = lease("case", &path);
    let new = lease("case", &path);
    assert!(!old.owns_failure_state());
    drop(new);
    assert!(!old.owns_failure_state());
    let c = rusqlite::Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE output(state TEXT); INSERT INTO output VALUES('ready')")
        .unwrap();
    let _fence = old.failure_fence_connection(&c);
    assert!(c.execute_batch("UPDATE output SET state='failed'").is_err());
    assert_eq!(
        c.query_row("SELECT state FROM output", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ready"
    );
}

#[test]
fn bound_workspace_pool_carries_reader_allocation_until_reader_drops() {
    let temp = tempfile::tempdir().unwrap();
    let path = source_file(&temp);
    let factory = build_factory(&path, 128 * MIB, true);
    let lease = SourcePublicationLease::acquire("case", "source", factory.clone());
    let root = ResourcePool::new(Resources {
        bytes: 1536 * MIB,
        workers: 4,
        ..Default::default()
    });
    let workspace = root.workspace(&AtomicBool::new(false)).unwrap();
    lease.bind_pool(workspace.pool());
    let reader = factory.open_reader().unwrap();
    assert_eq!(workspace.pool().snapshot().0.bytes, 128 * MIB);
    drop(workspace);
    assert_eq!(root.snapshot().0.bytes, 128 * MIB);
    drop(reader);
    assert_eq!(root.snapshot().0, Resources::default());
}
