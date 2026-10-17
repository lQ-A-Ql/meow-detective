//! Allocation ownership shared by import factories and preview E01 plans.
use crate::resource_budget::{global_pool, ResourceError, ResourcePermit, Resources, MIB};
use evidence_core::reader::{ReaderAdmission, ReaderAllocationScope, ReaderResourceHold};
use evidence_core::{EvidenceReader, ReaderInfo};
use std::{
    io,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

pub(crate) struct SharedPlan {
    pub plan: image_e01::E01OpenPlan,
    pub route: ResourceRoute,
}

#[derive(Clone, Default)]
pub(crate) struct ResourceRoute(Arc<Mutex<Option<Arc<dyn ReaderAllocationScope>>>>);
impl ResourceRoute {
    pub fn bind(&self, scope: Arc<dyn ReaderAllocationScope>) {
        *self.0.lock().unwrap_or_else(|p| p.into_inner()) = Some(scope);
    }
    pub fn scope(&self) -> Option<Arc<dyn ReaderAllocationScope>> {
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .filter(|s| s.active())
    }
}
pub(crate) struct WorkspaceAdmission(pub crate::resource_budget::ResourcePool);
impl ReaderAllocationScope for WorkspaceAdmission {
    fn active(&self) -> bool {
        self.0.active()
    }
    fn acquire(&self, bytes: u64, handles: u64) -> io::Result<Arc<dyn ReaderResourceHold>> {
        let reader = global_pool()
            .acquire(
                Resources {
                    readers: 1,
                    handles,
                    ..Default::default()
                },
                None,
            )
            .map_err(ResourceError::into_io)?;
        let memory = self
            .0
            .acquire(
                Resources {
                    bytes,
                    ..Default::default()
                },
                None,
            )
            .map_err(ResourceError::into_io)?;
        Ok(Arc::new((reader, memory)))
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
            .map_err(ResourceError::into_io)
    }
}
#[derive(Clone)]
pub(crate) struct ReaderBudget {
    pub bytes: u64,
    pub handles: u64,
    pub route: ResourceRoute,
}
impl ReaderAdmission for ReaderBudget {
    fn acquire(&self) -> io::Result<Arc<dyn ReaderResourceHold>> {
        if self.bytes > 192 * MIB {
            return Err(ResourceError::Limit.into_io());
        }
        if let Some(scope) = self.route.scope() {
            return scope.acquire(self.bytes, self.handles);
        }
        // Preview has no import cancellation token; its cache supplies a
        // case-scoped cancellation token through CaseReaderAdmission instead.
        global_pool()
            .acquire(
                Resources {
                    bytes: self.bytes,
                    readers: 1,
                    handles: self.handles,
                    ..Default::default()
                },
                None,
            )
            .map(|p| Arc::new(p) as Arc<dyn ReaderResourceHold>)
            .map_err(ResourceError::into_io)
    }
}
pub(crate) struct CaseReaderAdmission {
    pub budget: ReaderBudget,
    pub cancel: Arc<AtomicBool>,
}
impl ReaderAdmission for CaseReaderAdmission {
    fn acquire(&self) -> io::Result<Arc<dyn ReaderResourceHold>> {
        loop {
            if self.cancel.load(Ordering::Acquire) {
                return Err(ResourceError::Cancelled.into_io());
            }
            match self.budget.acquire() {
                Err(e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        && self.budget.route.scope().is_none() =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                result => return result,
            }
        }
    }
}
pub(crate) fn open_e01_plan(
    path: &Path,
    cancel: Option<&AtomicBool>,
    case_cancel: Option<Arc<AtomicBool>>,
    route: ResourceRoute,
) -> io::Result<image_e01::E01OpenPlan> {
    let mut handles = 0;
    for index in 1..=513 {
        if !image_e01::segment_path(path, index).try_exists()? {
            break;
        }
        handles += 1;
    }
    if handles == 0 || handles > 512 {
        return Err(ResourceError::Limit.into_io());
    }
    let parse = global_pool()
        .acquire(
            Resources {
                bytes: 896 * MIB,
                metadata: 256 * MIB,
                handles,
                ..Default::default()
            },
            cancel,
        )
        .map_err(ResourceError::into_io)?;
    let plan = image_e01::E01Reader::open_with_segment_limit(path, handles as u32)?.open_plan()?;
    let bytes = plan.metadata_bytes();
    let budget = ReaderBudget {
        bytes: plan.reader_memory_bound(),
        handles: plan.segment_count() as u64,
        route,
    };
    if budget.bytes > 192 * MIB {
        return Err(ResourceError::Limit.into_io());
    }
    let metadata = parse
        .shrink(Resources {
            bytes,
            metadata: bytes,
            ..Default::default()
        })
        .map_err(ResourceError::into_io)?;
    let admission: Arc<dyn ReaderAdmission> = match case_cancel {
        Some(cancel) => Arc::new(CaseReaderAdmission { budget, cancel }),
        None => Arc::new(budget),
    };
    Ok(plan.with_resources(Arc::new(metadata), admission))
}

pub(crate) struct BudgetedReader {
    pub reader: Box<dyn EvidenceReader>,
    pub _hold: Arc<dyn ReaderResourceHold>,
    pub _metadata: Arc<dyn ReaderResourceHold>,
}
impl std::io::Read for BudgetedReader {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.reader.read(b)
    }
}
impl std::io::Seek for BudgetedReader {
    fn seek(&mut self, p: std::io::SeekFrom) -> io::Result<u64> {
        self.reader.seek(p)
    }
}
impl EvidenceReader for BudgetedReader {
    fn info(&self) -> &ReaderInfo {
        self.reader.info()
    }
    fn preferred_read_granularity(&self) -> usize {
        self.reader.preferred_read_granularity()
    }
}
pub(crate) fn raw_parse(cancel: Option<&AtomicBool>) -> io::Result<ResourcePermit> {
    global_pool()
        .acquire(
            Resources {
                bytes: 384 * MIB,
                metadata: 256 * MIB,
                handles: 32,
                ..Default::default()
            },
            cancel,
        )
        .map_err(ResourceError::into_io)
}
