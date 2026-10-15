use std::collections::BTreeMap;
use std::sync::{Condvar, Mutex};

use fs_ntfs::mft_scanner::MftRecord;

#[derive(Debug)]
pub struct MftBatch {
    pub start_record: u64,
    pub scanned_records: u64,
    pub logical_bytes: u64,
    pub records: Vec<MftRecord>,
}

struct WindowState {
    in_flight: usize,
    next_record: u64,
    ready: BTreeMap<u64, MftBatch>,
}

/// Ordered, bounded batch window. It limits both outstanding tasks and reorder memory.
pub struct OrderedMftWindow {
    capacity: usize,
    state: Mutex<WindowState>,
    changed: Condvar,
}

impl OrderedMftWindow {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            state: Mutex::new(WindowState {
                in_flight: 0,
                next_record: 0,
                ready: BTreeMap::new(),
            }),
            changed: Condvar::new(),
        }
    }

    pub fn reserve(&self, _start_record: u64) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.in_flight >= self.capacity {
            return false;
        }
        state.in_flight += 1;
        true
    }

    pub fn submit(&self, batch: MftBatch) -> Result<(), MftBatch> {
        let _batch_shape = (batch.logical_bytes, batch.records.len());
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.ready.contains_key(&batch.start_record) {
            return Err(batch);
        }
        state.ready.insert(batch.start_record, batch);
        self.changed.notify_all();
        Ok(())
    }

    pub fn take_next(&self) -> Option<MftBatch> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let key = state.ready.keys().next().copied()?;
        if key != state.next_record {
            return None;
        }
        let batch = state.ready.remove(&key)?;
        state.next_record = state.next_record.saturating_add(batch.scanned_records);
        state.in_flight = state.in_flight.saturating_sub(1);
        self.changed.notify_all();
        Some(batch)
    }

    #[cfg(test)]
    pub fn outstanding(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .in_flight
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/parallel_enum/batch_pipeline.rs"]
mod tests;
