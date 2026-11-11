mod producer;
mod workers;

use crossbeam_channel::unbounded;
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use evidence_core::FileSystemReader;
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::{DbError, DbResult};
use producer::{DirectoryWork, ParallelBatchProducer, WorkerResult};
use rusqlite::Connection;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;
use workers::{cancellation_requested, run_workers, spawn_worker, WorkerRuntime};

pub fn enumerate_filesystem_parallel_with_factory(
    conn: &Connection,
    data_source_id: &DataSourceId,
    worker_count: usize,
    reader_factory: Arc<dyn Fn() -> DbResult<Box<dyn FileSystemReader>> + Send + Sync>,
    root_name_override: Option<&str>,
    progress_fn: Option<&dyn Fn(u32)>,
    cancel_token: Option<&AtomicBool>,
) -> DbResult<crate::file_service::EnumerationStats> {
    let root_reader = reader_factory()?;
    let root = root_reader
        .root()
        .map_err(|error| DbError::System(format!("Failed to read filesystem root: {error}")))?;
    drop(root_reader);
    if cancellation_requested(cancel_token) {
        return Err(DbError::System("Enumeration cancelled".to_string()));
    }

    let root_id = FileEntryId(Uuid::new_v4().to_string());
    let root_entry = FileEntry {
        id: root_id.clone(),
        parent_id: None,
        data_source_id: data_source_id.clone(),
        path: String::new(),
        name: root_name_override.unwrap_or(&root.name).to_string(),
        entry_type: EntryType::Directory,
        size: None,
        ext: None,
        deleted: false,
        hidden: root.hidden,
        system: root.system,
        encrypted: root.encrypted,
        read_only: root.read_only,
        archive: root.archive,
        unix_mode: root.unix_mode,
        created_at: root.created_at,
        modified_at: root.modified_at,
        accessed_at: root.accessed_at,
        changed_at: root.changed_at,
        hash_sha256: None,
    };

    if persistence_source::detail::open_store_for_write(conn)?.is_some() {
        return enumerate_native_parallel(
            conn,
            data_source_id,
            worker_count,
            reader_factory,
            root_entry,
            progress_fn,
            cancel_token,
        );
    }

    let worker_count = worker_count.max(1);
    let (work_tx, work_rx) = unbounded::<DirectoryWork>();
    let (result_tx, result_rx) = unbounded::<WorkerResult>();
    let stop = Arc::new(AtomicBool::new(false));
    let transaction = conn.unchecked_transaction()?;
    let repo = FileRepo::new(&transaction);
    repo.insert_batch_unchecked(&[root_entry])?;
    work_tx
        .send(DirectoryWork {
            parent_id: root_id,
            path: String::new(),
            depth: 0,
        })
        .map_err(|_| DbError::System("Parallel filesystem work queue closed".to_string()))?;
    let result = run_workers(
        &repo,
        data_source_id,
        worker_count,
        WorkerRuntime {
            reader_factory,
            work_tx,
            work_rx,
            result_tx,
            result_rx,
            progress_fn,
            cancel_token,
            stop,
        },
    );
    let stats = result?;
    transaction.commit()?;
    Ok(stats)
}

fn enumerate_native_parallel(
    conn: &Connection,
    data_source_id: &DataSourceId,
    worker_count: usize,
    reader_factory: Arc<dyn Fn() -> DbResult<Box<dyn FileSystemReader>> + Send + Sync>,
    root_entry: FileEntry,
    progress_fn: Option<&dyn Fn(u32)>,
    cancel_token: Option<&AtomicBool>,
) -> DbResult<crate::file_service::EnumerationStats> {
    let source_id = persistence_source::detail::source_id(conn)?
        .ok_or_else(|| DbError::System("source identity is missing".into()))?;
    if source_id != data_source_id.0 {
        return Err(DbError::System(
            "enumeration source identity mismatch".into(),
        ));
    }
    let worker_count = worker_count.max(1);
    let (work_tx, work_rx) = unbounded::<DirectoryWork>();
    let (result_tx, result_rx) = unbounded::<WorkerResult>();
    let stop = Arc::new(AtomicBool::new(false));
    work_tx
        .send(DirectoryWork {
            parent_id: root_entry.id.clone(),
            path: String::new(),
            depth: 0,
        })
        .map_err(|_| DbError::System("Parallel filesystem work queue closed".to_string()))?;
    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            spawn_worker(
                scope,
                work_rx.clone(),
                result_tx.clone(),
                Arc::clone(&reader_factory),
                Arc::clone(&stop),
            );
        }
        drop(result_tx);
        let store = persistence_source::detail::open_store_for_write(conn)?
            .ok_or_else(|| DbError::System("source detail storage is missing".into()))?;
        let repository = persistence_fjall::CatalogRecordStore::from_source_store((*store).clone());
        let root_record = persistence_source::detail::file_record(&root_entry);
        let mut producer = ParallelBatchProducer::new(
            work_tx,
            result_rx,
            progress_fn,
            cancel_token,
            Arc::clone(&stop),
        );
        let result = repository.replace_subtree_streamed(
            &source_id,
            &root_record,
            0,
            persistence_fjall::PersistMode::SyncAll,
            || {
                producer.next_batch(data_source_id).map(|batch| {
                    batch.map(|entries| {
                        entries
                            .iter()
                            .map(persistence_source::detail::file_record)
                            .collect()
                    })
                })
            },
        );
        if result.is_err() {
            stop.store(true, Ordering::Release);
        }
        match result {
            Ok(_) => Ok(producer.into_stats()),
            Err(persistence_fjall::CatalogStreamError::Storage(error)) => {
                Err(DbError::System(error.to_string()))
            }
            Err(persistence_fjall::CatalogStreamError::Producer(error)) => Err(error),
        }
    })
}
