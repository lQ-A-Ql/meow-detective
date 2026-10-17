use super::super::*;
use crate::resource_budget::{ResourcePool, Resources, MIB};
use evidence_core::reader::{ReaderAllocationScope, ReaderResourceHold};
use std::{
    io::{self, Cursor, Read, Seek, SeekFrom},
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};
struct Scope(ResourcePool);
impl ReaderAllocationScope for Scope {
    fn active(&self) -> bool {
        true
    }
    fn acquire(&self, _: u64, _: u64) -> io::Result<Arc<dyn ReaderResourceHold>> {
        Ok(Arc::new(()))
    }
    fn acquire_work(&self, bytes: u64, workers: u64) -> io::Result<Arc<dyn ReaderResourceHold>> {
        self.0
            .acquire(
                Resources {
                    bytes,
                    workers,
                    ..Default::default()
                },
                None,
            )
            .map(|p| Arc::new(p) as Arc<dyn ReaderResourceHold>)
            .map_err(|e| e.into_io())
    }
}
struct Reader {
    bytes: Cursor<Arc<[u8]>>,
    info: evidence_core::ReaderInfo,
    active: Arc<AtomicUsize>,
    slow: bool,
    fail: bool,
}
impl Read for Reader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.active.fetch_add(1, Ordering::AcqRel);
        if self.slow {
            std::thread::sleep(Duration::from_millis(3));
        }
        let result = if self.fail && self.bytes.position() > 1024 * 1024 {
            Err(io::Error::other("injected read failure"))
        } else {
            self.bytes.read(bytes)
        };
        self.active.fetch_sub(1, Ordering::AcqRel);
        result
    }
}
impl Seek for Reader {
    fn seek(&mut self, at: SeekFrom) -> io::Result<u64> {
        self.bytes.seek(at)
    }
}
impl evidence_core::EvidenceReader for Reader {
    fn info(&self) -> &evidence_core::ReaderInfo {
        &self.info
    }
}
struct Factory {
    bytes: Arc<[u8]>,
    opened: AtomicUsize,
    active: Arc<AtomicUsize>,
    fail: bool,
    scope: Arc<Scope>,
}
impl EvidenceReaderFactory for Factory {
    fn open_reader(&self) -> io::Result<Box<dyn evidence_core::EvidenceReader>> {
        Ok(Box::new(Reader {
            bytes: Cursor::new(self.bytes.clone()),
            info: evidence_core::ReaderInfo {
                path: "memory".into(),
                kind: "raw".into(),
                size: self.bytes.len() as u64,
            },
            slow: self.opened.fetch_add(1, Ordering::AcqRel) == 0,
            fail: self.fail,
            active: self.active.clone(),
        }))
    }
    fn logical_size(&self) -> u64 {
        self.bytes.len() as u64
    }
    fn preferred_read_granularity(&self) -> usize {
        512
    }
    fn capabilities(&self) -> evidence_core::ReaderCapabilities {
        evidence_core::ReaderCapabilities {
            max_concurrent_readers: 4,
            ..Default::default()
        }
    }
    fn reader_scope(&self) -> Option<Arc<dyn ReaderAllocationScope>> {
        Some(self.scope.clone())
    }
}
#[test]
fn skew_pause_cancel_and_read_or_sink_failure_join_workers_and_return_window_bytes() {
    let bytes: Arc<[u8]> = super::resource_grid::fixture::image(40_003).into();
    for mode in 0..6 {
        let pool = ResourcePool::new(Resources {
            bytes: 256 * MIB,
            workers: 4,
            ..Default::default()
        });
        let factory = Factory {
            bytes: bytes.clone(),
            opened: AtomicUsize::new(0),
            active: Arc::new(AtomicUsize::new(0)),
            fail: mode == 4,
            scope: Arc::new(Scope(pool.clone())),
        };
        let mut profile = PhaseRecorder::new(None, Some(0));
        let mut scan = super::super::super::mft_scan::prepare_mft_scan_from_reader(
            factory.open_reader().unwrap(),
            0,
            &mut profile,
        )
        .unwrap();
        let cancel = AtomicBool::new(false);
        let mut next = 0;
        let cancel_after = 1 + ((mode as u64 * 1_103_515_245 + 12_345) % 4);
        let result = collect_batches_with_budget(
            &mut scan,
            Some(&factory),
            4,
            &cancel,
            &mut profile,
            256 * MIB,
            |batch, _| {
                assert_eq!(batch.start_record, next);
                next += batch.scanned_records;
                if mode == 5 && next > 10_000 {
                    return Err(ParallelEnumError::Db(rusqlite::Error::InvalidQuery));
                }
                std::thread::sleep(Duration::from_millis(2));
                if mode > 0 && mode < 4 && next >= cancel_after * 10_000 {
                    cancel.store(true, Ordering::Release);
                }
                Ok(())
            },
        );
        if mode == 0 {
            result.unwrap();
            assert_eq!(next, 40_003);
        } else if mode == 4 {
            assert!(matches!(result, Err(ParallelEnumError::Io(_))));
        } else if mode == 5 {
            assert!(matches!(result, Err(ParallelEnumError::Db(_))));
        } else {
            assert!(matches!(result, Err(ParallelEnumError::Cancelled)));
        }
        assert_eq!(factory.opened.load(Ordering::Acquire), 4);
        assert_eq!(factory.active.load(Ordering::Acquire), 0);
        assert!(pool.snapshot().1.bytes <= 256 * MIB);
        assert!(pool.snapshot().1.workers <= 4);
        drop(scan);
        assert_eq!(pool.snapshot().0, Resources::default());
    }
}

#[test]
fn oversized_record_zero_is_rejected_before_allocating_a_hostile_boot_geometry() {
    let mut bytes = super::resource_grid::fixture::image(32);
    bytes[0x40] = (-31i8) as u8;
    let reader = Reader {
        info: evidence_core::ReaderInfo {
            path: "memory".into(),
            kind: "raw".into(),
            size: bytes.len() as u64,
        },
        bytes: Cursor::new(Arc::from(bytes)),
        active: Arc::new(AtomicUsize::new(0)),
        slow: false,
        fail: false,
    };
    let result = super::super::super::mft_scan::prepare_mft_scan_from_reader(
        Box::new(reader),
        0,
        &mut PhaseRecorder::new(None, Some(0)),
    );
    assert!(matches!(
        result,
        Err(ParallelEnumError::MftResourceLimit(_))
    ));
}
