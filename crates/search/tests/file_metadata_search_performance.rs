use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};

use search::{FileSearchAfterKey, FileSearchOptions, SearchFileDocument, SearchIndex};

const DOCUMENT_COUNT: usize = 100_000;
const INDEX_BATCH_SIZE: usize = 1_000;
const PAGE_SIZE: usize = 2_048;

#[test]
#[ignore = "synthetic 100k metadata index performance baseline"]
fn metadata_search_100k_performance_and_cursor_integrity() {
    let directory = tempfile::tempdir().unwrap();
    let index = SearchIndex::create(directory.path()).unwrap();
    let mut writer = index.metadata_writer().unwrap();
    let index_started = Instant::now();
    for batch_start in (0..DOCUMENT_COUNT).step_by(INDEX_BATCH_SIZE) {
        let documents = (batch_start..batch_start + INDEX_BATCH_SIZE)
            .map(performance_document)
            .collect::<Vec<_>>();
        writer.add_documents(&documents).unwrap();
    }
    writer.commit().unwrap();
    let index_elapsed = index_started.elapsed();

    let options = FileSearchOptions {
        query: "report-099".to_string(),
        ..Default::default()
    };
    let mut query_samples = Vec::with_capacity(40);
    for _ in 0..40 {
        let started = Instant::now();
        let page = index
            .file_query_session(&options)
            .unwrap()
            .rank_after(None, 100)
            .unwrap();
        assert_eq!(page.total_count, 1_000);
        query_samples.push(started.elapsed());
    }
    query_samples.sort_unstable();

    let cursor_started = Instant::now();
    let session = index
        .file_query_session(&FileSearchOptions::default())
        .unwrap();
    let mut after: Option<FileSearchAfterKey> = None;
    let mut ids = HashSet::with_capacity(DOCUMENT_COUNT);
    loop {
        let page = session.rank_after(after.as_ref(), PAGE_SIZE).unwrap();
        if page.hits.is_empty() {
            break;
        }
        after = page.hits.last().map(|hit| hit.after_key());
        for hit in page.hits {
            assert!(
                ids.insert(hit.file_id().to_string()),
                "duplicate cursor hit"
            );
        }
    }
    assert_eq!(ids.len(), DOCUMENT_COUNT);
    let cursor_elapsed = cursor_started.elapsed();

    let index_bytes = directory_size(directory.path());
    let peak_working_set_bytes = peak_working_set_bytes();
    println!(
        "FILE_METADATA_SEARCH_BENCHMARK_JSON={}",
        serde_json::json!({
            "documentCount": DOCUMENT_COUNT,
            "indexElapsedMs": index_elapsed.as_millis(),
            "indexBytes": index_bytes,
            "queryP50Us": percentile(&query_samples, 50).as_micros(),
            "queryP95Us": percentile(&query_samples, 95).as_micros(),
            "fullCursorIntegrityMs": cursor_elapsed.as_millis(),
            "peakWorkingSetBytes": peak_working_set_bytes,
        })
    );
}

fn performance_document(index: usize) -> SearchFileDocument {
    SearchFileDocument {
        file_id: format!("file-{index:06}"),
        path: format!(
            "/Users/investigator/Documents/{:03}/report-{index:06}.dat",
            index / 1_000
        ),
        name: format!("report-{index:06}.dat"),
        extension: "dat".to_string(),
        entry_type: "file".to_string(),
        size: Some((index as u64).saturating_mul(4_096)),
        modified_at: Some(1_700_000_000_000_i64.saturating_add(index as i64)),
        deleted: index.is_multiple_of(101),
        hidden: false,
        system: false,
        encrypted: index.is_multiple_of(257),
    }
}

fn percentile(samples: &[Duration], percentile: usize) -> Duration {
    let index = samples
        .len()
        .saturating_mul(percentile)
        .div_ceil(100)
        .saturating_sub(1)
        .min(samples.len().saturating_sub(1));
    samples[index]
}

fn directory_size(path: &Path) -> u64 {
    std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(|kind| kind.is_dir())
                .map_or_else(
                    || entry.metadata().map(|metadata| metadata.len()).unwrap_or(0),
                    |_| directory_size(&entry.path()),
                )
        })
        .sum()
}

#[cfg(windows)]
fn peak_working_set_bytes() -> Option<u64> {
    #[repr(C)]
    struct ProcessMemoryCounters {
        size: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
    }
    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            counters: *mut ProcessMemoryCounters,
            size: u32,
        ) -> i32;
    }

    let mut counters = ProcessMemoryCounters {
        size: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };
    // SAFETY: Windows returns a pseudo-handle valid for this process, and the
    // initialized structure and byte length match PROCESS_MEMORY_COUNTERS.
    let succeeded =
        unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.size) };
    (succeeded != 0).then_some(counters.peak_working_set_size as u64)
}

#[cfg(not(windows))]
fn peak_working_set_bytes() -> Option<u64> {
    None
}
