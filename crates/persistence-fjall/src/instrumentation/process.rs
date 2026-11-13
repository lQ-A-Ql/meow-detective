use super::StorageCounters;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

static ACTIVE: Mutex<Option<Arc<Mutex<StorageCounters>>>> = Mutex::new(None);
static ENABLED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, thiserror::Error)]
#[error("a process measurement is already active")]
pub struct MeasurementBusy;

/// Collects process-wide work until dropped. Join workers before taking the
/// final snapshot; use an isolated process to exclude unrelated activity.
pub struct ProcessMeasurement {
    counters: Arc<Mutex<StorageCounters>>,
}

impl ProcessMeasurement {
    pub fn start() -> Result<Self, MeasurementBusy> {
        let mut active = ACTIVE.lock().expect("measurement registry poisoned");
        if active.is_some() {
            return Err(MeasurementBusy);
        }
        let counters = Arc::new(Mutex::new(StorageCounters::default()));
        *active = Some(counters.clone());
        ENABLED.store(true, Ordering::Release);
        Ok(Self { counters })
    }

    pub fn snapshot(&self) -> StorageCounters {
        *self.counters.lock().expect("measurement counters poisoned")
    }
}

impl Drop for ProcessMeasurement {
    fn drop(&mut self) {
        let mut active = ACTIVE.lock().expect("measurement registry poisoned");
        if active
            .as_ref()
            .is_some_and(|counters| Arc::ptr_eq(counters, &self.counters))
        {
            ENABLED.store(false, Ordering::Release);
            active.take();
        }
    }
}

pub(super) fn record(update: impl FnOnce(&mut StorageCounters)) {
    if !ENABLED.load(Ordering::Acquire) {
        return;
    }
    let counters = ACTIVE
        .lock()
        .expect("measurement registry poisoned")
        .clone();
    if let Some(counters) = counters {
        update(&mut counters.lock().expect("measurement counters poisoned"));
    }
}
