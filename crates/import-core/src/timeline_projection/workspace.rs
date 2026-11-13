use super::{fjall, TimelineProjectionError};
use domain::DataSourcePlatform;
use persistence_source::fjall::{
    CatalogRecordStore, ScanDirection, SourceStore, TimelineRecord, TimelineRecordStore,
};
use std::sync::atomic::{AtomicBool, Ordering};
use timeline::TimelineProjectionPolicy;

mod counts;
use counts::CountWorkspace;

const MIB: u64 = 1024 * 1024;
const BASE_WORKSPACE_BYTES: u64 = 64 * MIB;
const MAX_ENGINE_ALLOWANCE_BYTES: u64 = 160 * MIB;
const ENGINE_ALLOWANCE_PER_MUTATION: u64 = 4096;
const ENTRY_CONTAINER_BYTES: u64 = 96;
const GRAPH_PAGE_ROWS: u64 = 512;
const PAGE_ROWS: usize = 512;

// The base covers the 32 MiB source block cache and bounded read/SQLite work.
// The engine allowance grows with mutations; transaction payload and container
// estimates continue growing after that allowance reaches its cap. Coefficients
// include headroom over the measured 50K single-event projection (224 MiB delta).
#[derive(Debug, Clone, Default)]
pub(crate) struct WorkspaceEstimate {
    pub catalog_records: u64,
    pub eligible_files: u64,
    pub projected_events: u64,
    pub removed_events: u64,
    pub graph_events: u64,
    pub mutation_bytes: u64,
    pub mutation_entries: u64,
    pub newly_written_bytes: u64,
    pub max_timeline_record_bytes: u64,
    pub max_graph_row_bytes: u64,
    pub required_bytes: u64,
    pub count_key_upper_bound: u64,
    counts: CountWorkspace,
}

pub(crate) fn estimate(
    store: &SourceStore,
    source: &str,
    platform: DataSourcePlatform,
    cancel: &AtomicBool,
    replace_events: bool,
    project_graph: bool,
) -> Result<WorkspaceEstimate, TimelineProjectionError> {
    let snapshot = store.database().read_snapshot();
    let mut result = WorkspaceEstimate::default();
    if replace_events {
        let catalog = CatalogRecordStore::from_source_store(store.clone());
        let mut cursor = None;
        loop {
            active(cancel)?;
            let page = catalog.list_all(
                &snapshot,
                source,
                cursor.as_deref(),
                ScanDirection::Forward,
                PAGE_ROWS,
            )?;
            for record in page.records {
                active(cancel)?;
                add(&mut result.catalog_records, 1)?;
                if fjall::eligible(&record, platform) {
                    add(&mut result.eligible_files, 1)?;
                    for event in fjall::file_events_for_record(record, source, platform) {
                        result.inserted(&event, project_graph)?;
                    }
                }
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
    }
    let timeline = TimelineRecordStore::from_source_store(store.clone());
    let mut cursor = None;
    loop {
        active(cancel)?;
        let page = timeline.list_by_source(
            &snapshot,
            source,
            cursor.as_deref(),
            ScanDirection::Forward,
            PAGE_ROWS,
        )?;
        for record in page.records {
            active(cancel)?;
            let replaced = replace_events
                && record
                    .parser_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with("timeline.file_"));
            if replaced {
                result.removed(&record)?;
            } else if project_graph {
                let cost = TimelineRecordStore::record_storage_cost(&record)?;
                result.graph(&record, cost.write_bytes)?;
            }
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    result.finish()?;
    Ok(result)
}

impl WorkspaceEstimate {
    fn inserted(
        &mut self,
        record: &TimelineRecord,
        graph: bool,
    ) -> Result<(), TimelineProjectionError> {
        let cost = TimelineRecordStore::record_storage_cost(record)?;
        add(&mut self.projected_events, 1)?;
        add(&mut self.mutation_bytes, cost.write_bytes)?;
        add(&mut self.mutation_entries, cost.entries)?;
        self.counts.observe(record)?;
        if graph {
            add(&mut self.newly_written_bytes, cost.write_bytes)?;
            self.graph(record, cost.write_bytes)?;
        }
        Ok(())
    }

    fn removed(&mut self, record: &TimelineRecord) -> Result<(), TimelineProjectionError> {
        let cost = TimelineRecordStore::record_storage_cost(record)?;
        add(&mut self.removed_events, 1)?;
        add(&mut self.mutation_bytes, cost.delete_bytes)?;
        add(&mut self.mutation_entries, cost.entries)?;
        self.counts.observe(record)?;
        Ok(())
    }

    fn graph(
        &mut self,
        record: &TimelineRecord,
        write_bytes: u64,
    ) -> Result<(), TimelineProjectionError> {
        add(&mut self.graph_events, 1)?;
        self.max_timeline_record_bytes = self.max_timeline_record_bytes.max(write_bytes);
        let strings = record.id.len() as u64 * 4
            + record.source_object_id.len() as u64 * 2
            + record.title.len() as u64
            + record.event_type.len() as u64 * 3;
        self.max_graph_row_bytes = self
            .max_graph_row_bytes
            .max(strings.checked_add(512).ok_or_else(overflow)?);
        Ok(())
    }

    fn finish(&mut self) -> Result<(), TimelineProjectionError> {
        let events = self
            .projected_events
            .checked_add(self.removed_events)
            .ok_or_else(overflow)?;
        let engine = events
            .saturating_mul(ENGINE_ALLOWANCE_PER_MUTATION)
            .min(MAX_ENGINE_ALLOWANCE_BYTES);
        let containers = self
            .mutation_entries
            .checked_mul(ENTRY_CONTAINER_BYTES)
            .ok_or_else(overflow)?;
        let (count_keys, count_bytes) = self.counts.required()?;
        self.count_key_upper_bound = count_keys;
        let transaction = self
            .mutation_bytes
            .checked_add(containers)
            .and_then(|bytes| bytes.checked_add(count_bytes))
            .ok_or_else(overflow)?;
        let graph_buffers = self
            .max_graph_row_bytes
            .checked_mul(GRAPH_PAGE_ROWS * 2)
            .ok_or_else(overflow)?;
        let read_page = self
            .max_timeline_record_bytes
            .checked_mul(GRAPH_PAGE_ROWS)
            .ok_or_else(overflow)?;
        // Existing timeline rows are read one page at a time. Only this
        // projection's committed writes contribute to retained engine bytes.
        let graph = self
            .newly_written_bytes
            .checked_add(graph_buffers)
            .and_then(|bytes| bytes.checked_add(read_page))
            .ok_or_else(overflow)?;
        self.required_bytes = BASE_WORKSPACE_BYTES
            .checked_add(engine)
            .and_then(|value| value.checked_add(transaction.max(graph)))
            .ok_or_else(overflow)?;
        Ok(())
    }

    pub fn require(&self, policy: TimelineProjectionPolicy) -> Result<(), TimelineProjectionError> {
        let available_bytes = policy.reservation_bytes()?;
        if self.required_bytes > available_bytes {
            return Err(TimelineProjectionError::WorkspaceLimit {
                required_bytes: self.required_bytes,
                available_bytes,
                projected_events: self.projected_events,
                removed_events: self.removed_events,
                graph_events: self.graph_events,
                mutation_bytes: self.mutation_bytes,
                mutation_entries: self.mutation_entries,
            });
        }
        Ok(())
    }
}

fn add(value: &mut u64, amount: u64) -> Result<(), TimelineProjectionError> {
    *value = value.checked_add(amount).ok_or_else(overflow)?;
    Ok(())
}

fn overflow() -> TimelineProjectionError {
    TimelineProjectionError::InvalidInput("timeline workspace estimate overflowed".to_owned())
}

fn active(cancel: &AtomicBool) -> Result<(), TimelineProjectionError> {
    if cancel.load(Ordering::Acquire) {
        Err(TimelineProjectionError::Cancelled)
    } else {
        Ok(())
    }
}
