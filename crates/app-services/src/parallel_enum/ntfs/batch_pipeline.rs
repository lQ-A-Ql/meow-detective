use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use fs_ntfs::mft_scanner::MftRecord;

#[derive(Debug)]
pub struct MftBatch {
    pub start_record: u64,
    pub scanned_records: u64,
    pub logical_bytes: u64,
    pub records: Vec<MftRecord>,
}

struct WindowState {
    reserved: BTreeSet<u64>,
    next_record: u64,
    ready: BTreeMap<u64, MftBatch>,
}

/// Ordered, bounded batch window. It limits both outstanding tasks and reorder memory.
pub struct OrderedMftWindow {
    capacity: usize,
    state: Mutex<WindowState>,
}

impl OrderedMftWindow {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            state: Mutex::new(WindowState {
                reserved: BTreeSet::new(),
                next_record: 0,
                ready: BTreeMap::new(),
            }),
        }
    }

    pub fn reserve(&self, start_record: u64) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.reserved.len() >= self.capacity || start_record < state.next_record {
            return false;
        }
        state.reserved.insert(start_record)
    }

    pub fn submit(&self, batch: MftBatch) -> Result<(), MftBatch> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if batch.scanned_records == 0
            || !state.reserved.contains(&batch.start_record)
            || state.ready.contains_key(&batch.start_record)
        {
            return Err(batch);
        }
        state.ready.insert(batch.start_record, batch);
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
        state.reserved.remove(&key);
        Some(batch)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/parallel_enum/batch_pipeline.rs"]
mod tests;
