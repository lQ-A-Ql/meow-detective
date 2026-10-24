use app_services::import_pipeline::ImportEventSink;
use rusqlite::Connection;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Mutex, time::Instant};
use transport::dto::{
    AnalysisExtractionProgressDto, DataSourceSummaryDto, ImportPhaseProgressDto,
    IndexCacheStatusDto, JobCancellationDto, PartialResultDto,
};

#[derive(Default)]
pub struct ProgressSink {
    last: Mutex<Option<Instant>>,
}

impl ProgressSink {
    fn emit(&self, detail: &str) {
        let mut last = self.last.lock().unwrap();
        if last.is_none_or(|last| last.elapsed().as_secs() >= 2) {
            eprintln!("ACCEPTANCE_PROGRESS {detail}");
            *last = Some(Instant::now());
        }
    }
    pub fn analysis(&self, progress: &AnalysisExtractionProgressDto) {
        self.emit(&format!(
            "{:?} {}/{} {}",
            progress.phase,
            progress.processed_candidates,
            progress.total_candidates,
            progress.current_path.as_deref().unwrap_or("")
        ));
    }
}

impl ImportEventSink for ProgressSink {
    fn job_progress(&self, _: &str, _: u32, detail: &str) {
        self.emit(detail);
    }
    fn partition_progress(&self, _: &str, partition: &str, _: u32, _: u32, percent: u32) {
        self.emit(&format!("{partition} {percent}%"));
    }
    fn timeline_updated(&self, _: u64) {}
    fn search_index_progress(&self, _: u32, detail: &str) {
        self.emit(detail);
    }
    fn data_source_imported(&self, _: &str, _: &DataSourceSummaryDto, _: &str) {}
    fn import_phase_progress(&self, progress: &ImportPhaseProgressDto) {
        self.emit(&progress.detail);
    }
    fn import_partial_result(&self, _: &PartialResultDto) {}
    fn cache_index_status(&self, _: &IndexCacheStatusDto) {}
    fn job_cancellation(&self, _: &JobCancellationDto) {}
}

pub fn write_json(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

pub fn artifact_digest(source: &Connection) -> String {
    let mut statement = source.prepare("SELECT extractor_id, source_object_id, attrs FROM artifacts ORDER BY extractor_id, source_object_id, attrs").unwrap();
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap();
    let mut hash = Sha256::new();
    for row in rows {
        let (producer, file, attrs) = row.unwrap();
        hash.update(serde_json::to_vec(&(producer, file, attrs)).unwrap());
    }
    hex::encode(hash.finalize())
}

pub fn catalog(source: &Connection) -> Value {
    let (rows, files, directories): (u64,u64,u64) = source.query_row("SELECT COUNT(*),SUM(entry_type='file' COLLATE NOCASE),SUM(entry_type='directory' COLLATE NOCASE) FROM file_entries", [], |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    let orphans: u64 = source.query_row("SELECT COUNT(*) FROM file_entries f LEFT JOIN file_entries p ON p.id=f.parent_id WHERE f.parent_id IS NOT NULL AND p.id IS NULL", [], |row|row.get(0)).unwrap();
    assert!(files > 0 && directories > 0);
    assert_eq!(orphans, 0);
    json!({"rows":rows,"files":files,"directories":directories,"orphans":orphans})
}

pub fn process_counters() -> (u64, u64) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::{
            GetCurrentProcess, GetProcessIoCounters, IO_COUNTERS,
        };
        let mut counters = IO_COUNTERS::default();
        // SAFETY: the current-process handle is valid and the output pointer
        // references the correctly sized, initialized Windows ABI structure.
        assert_ne!(
            unsafe { GetProcessIoCounters(GetCurrentProcess(), &mut counters) },
            0
        );
        (counters.ReadTransferCount, counters.WriteTransferCount)
    }
    #[cfg(not(windows))]
    {
        (0, 0)
    }
}
